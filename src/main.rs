//! `mcp-obsidian` — CLI host and MCP server for an Obsidian markdown vault.
//!
//! Two ways to use the same retrieval core:
//!
//! * `ask` / `chat`  — local host loop: prompt → Ollama → vault tools → answer.
//! * `serve`         — expose the tools to external MCP hosts over stdio.
//!
//! Logging always goes to **stderr**; stdout is reserved for CLI output and, in
//! `serve` mode, for the JSON-RPC MCP stream.

//! `mcp-obsidian` CLI — a thin front end over the library crate.
//!
//! Two ways to use the same retrieval core:
//!
//! * `ask` / `chat`  — local host loop: prompt → Ollama → vault tools → answer.
//! * `serve`         — expose the tools to external MCP hosts over stdio.
//!
//! All behaviour lives in [`mcp_obsidian`]; this file only parses arguments and
//! prints results. Logging always goes to **stderr**; stdout is reserved for
//! CLI output and, in `serve` mode, for the JSON-RPC MCP stream.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use rmcp::ServiceExt;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tracing_subscriber::EnvFilter;

use mcp_obsidian::host::{self, Host};
use mcp_obsidian::mcp_server::{
    GetNoteRequest, LinkedNotesRequest, ObsidianServer, SearchByTagRequest, SearchByTermRequest,
    run_get_linked_notes, run_get_note, run_search_by_tag, run_search_by_term,
};
use mcp_obsidian::ollama_client::{ApiFlavor, DEFAULT_BASE_URL, DEFAULT_MODEL, OllamaClient};
use mcp_obsidian::ui::{self, theme};
use mcp_obsidian::vault::{DEFAULT_BODY_CHARS, DEFAULT_LIMIT, Vault};
/// Query kinds understood by the `search` command.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum SearchKind {
    /// Look notes up by tag.
    Tag,
    /// Full-text lookup across title, tags, headings, frontmatter and body.
    Term,
    /// Traverse the wikilink graph from a note.
    Links,
    /// Read one note in full.
    Note,
}

#[derive(Debug, Parser)]
#[command(
    name = "mcp-obsidian",
    version,
    about = "Deterministic, tag- and link-aware retrieval over an Obsidian vault, exposed to LLMs and MCP hosts."
)]
struct Cli {
    /// Obsidian vault directory to index (read-only).
    #[arg(long, global = true, env = "OBSIDIAN_VAULT", default_value = ".")]
    vault: PathBuf,

    /// Ollama model used for the host loop.
    #[arg(long, global = true, env = "OLLAMA_MODEL", default_value = DEFAULT_MODEL)]
    model: String,

    /// Ollama base URL (with or without the /v1 suffix).
    #[arg(long, global = true, env = "OLLAMA_URL", default_value = DEFAULT_BASE_URL)]
    ollama_url: String,

    /// Which Ollama API shape to use.
    #[arg(long, global = true, value_enum, default_value_t = ApiFlavor::Openai)]
    api: ApiFlavor,

    /// Increase log verbosity (repeat for debug logs).
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check Ollama reachability, model availability and vault indexing.
    Doctor,

    /// Print the vault index (notes, tags, links) without contacting a model.
    Index {
        /// Maximum number of notes to list (0 = all).
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },

    /// Query the vault directly, without a model in the loop.
    Search {
        /// What kind of lookup to run.
        #[arg(value_enum)]
        kind: SearchKind,

        /// Tags, terms, or the note name, depending on `kind`.
        #[arg(required = true)]
        query: Vec<String>,

        /// Require all tags/terms to match instead of any of them.
        #[arg(long)]
        match_all: bool,

        /// Maximum number of results.
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,

        /// Wikilink hops to traverse (only for `links`).
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Graph direction for `links`: outgoing, backlinks or both.
        #[arg(long, default_value = "both")]
        direction: String,

        /// Maximum characters of body text per result.
        #[arg(long, default_value_t = 400)]
        max_chars: usize,
    },

    /// Print the MCP tool definitions (JSON Schema) as JSON.
    Tools,

    /// Send one prompt to the model and print the answer.
    Ask {
        /// Prompt text.
        #[arg(required = true)]
        prompt: Vec<String>,

        /// Skip tool calling and just round-trip the prompt.
        #[arg(long)]
        no_tools: bool,

        /// Skip the deterministic pre-search of the vault.
        #[arg(long)]
        no_prefetch: bool,

        /// Maximum model rounds before giving up.
        #[arg(long, default_value_t = host::DEFAULT_MAX_ROUNDS)]
        max_rounds: usize,

        /// Print the pre-search and every tool call the model made to stderr.
        #[arg(long)]
        trace: bool,
    },

    /// Interactive prompt loop with vault tools enabled.
    Chat {
        /// Skip tool calling and just chat.
        #[arg(long)]
        no_tools: bool,

        /// Skip the deterministic pre-search of the vault.
        #[arg(long)]
        no_prefetch: bool,

        /// Maximum model rounds per prompt.
        #[arg(long, default_value_t = host::DEFAULT_MAX_ROUNDS)]
        max_rounds: usize,
    },

    /// Serve the retrieval tools to MCP hosts: stdio by default, Streamable
    /// HTTP with `--http`.
    Serve {
        /// Serve Streamable HTTP on this address instead of stdio. `--http` alone
        /// uses 127.0.0.1:8000; pass e.g. `0.0.0.0:8000` to expose it.
        #[arg(
            long,
            value_name = "ADDR",
            num_args = 0..=1,
            default_missing_value = "127.0.0.1:8000"
        )]
        http: Option<String>,

        /// Also accept this `Host` header in HTTP mode (repeatable). Loopback is
        /// always accepted; add your public hostname, e.g.
        /// `--allow-host vault.example.com`.
        #[arg(long, value_name = "HOST", requires = "http")]
        allow_host: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    match run(&cli).await {
        Err(error) if closed_stdout(&error) => Ok(()),
        other => other,
    }
}

/// Dispatch one CLI invocation.
async fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Serve { http, allow_host } => match http {
            Some(address) => run_serve_http(cli, address, allow_host).await,
            None => run_serve(cli).await,
        },
        Command::Doctor => run_doctor(cli).await,
        Command::Index { limit } => {
            let vault = load_vault(cli)?;
            ui::print_index(&mut ui::stdout_console(), &vault, *limit)?;
            Ok(())
        }
        Command::Search {
            kind,
            query,
            match_all,
            limit,
            depth,
            direction,
            max_chars,
        } => {
            let vault = load_vault(cli)?;
            let output = run_search(
                &vault, *kind, query, *match_all, *limit, *depth, direction, *max_chars,
            );
            ui::print_search(
                &mut ui::stdout_console(),
                kind_label(*kind),
                &query.join(" "),
                &output,
            )?;
            Ok(())
        }
        Command::Tools => {
            let specs = ObsidianServer::tool_specs();
            println!("{}", serde_json::to_string_pretty(&specs)?);
            Ok(())
        }
        Command::Ask {
            prompt,
            no_tools,
            no_prefetch,
            max_rounds,
            trace,
        } => {
            let host = build_host(cli, !*no_tools, !*no_prefetch, *max_rounds)?;
            let outcome = host.ask(&prompt.join(" ")).await?;
            if *trace {
                let mut console = ui::stderr_console();
                if let Some(query) = &outcome.prefetch {
                    ui::print_prefetch(&mut console, query)?;
                }
                ui::print_trace(&mut console, &outcome.trace)?;
                ui::print_summary(&mut console, outcome.rounds, outcome.tool_calls)?;
            }
            ui::print_answer(&mut ui::stdout_console(), outcome.answer.trim())?;
            Ok(())
        }
        Command::Chat {
            no_tools,
            no_prefetch,
            max_rounds,
        } => {
            let host = build_host(cli, !*no_tools, !*no_prefetch, *max_rounds)?;
            host.chat_loop().await
        }
    }
}

/// Human-readable label for a search kind, used in the result header.
fn kind_label(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Tag => "tags",
        SearchKind::Term => "terms",
        SearchKind::Links => "links",
        SearchKind::Note => "note",
    }
}

/// `cmd | head` closes stdout early; that is not an error worth reporting.
fn closed_stdout(error: &anyhow::Error) -> bool {
    match error.downcast_ref::<richrs::errors::Error>() {
        Some(rich_error) => is_broken_pipe(rich_error),
        None => false,
    }
}

/// Whether a richrs I/O error is a broken pipe.
fn is_broken_pipe(error: &richrs::errors::Error) -> bool {
    match error {
        richrs::errors::Error::ConsoleIo { source } => {
            source.kind() == std::io::ErrorKind::BrokenPipe
        }
        _ => false,
    }
}

/// Dispatch a `search` subcommand to the shared tool implementation.
#[allow(clippy::too_many_arguments)]
fn run_search(
    vault: &Vault,
    kind: SearchKind,
    query: &[String],
    match_all: bool,
    limit: usize,
    depth: usize,
    direction: &str,
    max_chars: usize,
) -> String {
    match kind {
        SearchKind::Tag => run_search_by_tag(
            vault,
            &SearchByTagRequest {
                tags: query.to_vec(),
                match_all,
                limit,
                max_chars,
            },
        ),
        SearchKind::Term => run_search_by_term(
            vault,
            &SearchByTermRequest {
                terms: query.to_vec(),
                match_all,
                limit,
                max_chars,
            },
        ),
        SearchKind::Links => run_get_linked_notes(
            vault,
            &LinkedNotesRequest {
                note: query.join(" "),
                depth,
                direction: direction.to_string(),
                limit,
                max_chars,
            },
        ),
        SearchKind::Note => run_get_note(
            vault,
            &GetNoteRequest {
                note: query.join(" "),
                max_chars: if max_chars == 400 {
                    DEFAULT_BODY_CHARS
                } else {
                    max_chars
                },
            },
        ),
    }
}

/// Build a host loop from the CLI configuration.
fn build_host(cli: &Cli, use_tools: bool, use_prefetch: bool, max_rounds: usize) -> Result<Host> {
    let vault = Arc::new(load_vault(cli)?);
    let client = OllamaClient::new(&cli.ollama_url, cli.api, &cli.model)?;
    let server = Arc::new(ObsidianServer::new(vault));
    let mut host = Host::new(client, server).with_max_rounds(max_rounds);
    if !use_tools {
        host = host.without_tools();
    }
    if !use_prefetch {
        host = host.without_prefetch();
    }
    Ok(host)
}

/// Index the configured vault, logging what was found.
fn load_vault(cli: &Cli) -> Result<Vault> {
    let vault = Vault::load(&cli.vault)?;
    if vault.notes().is_empty() {
        tracing::warn!(
            vault = %vault.root().display(),
            "no markdown notes found in the vault"
        );
    } else {
        let stats = vault.stats();
        tracing::info!(
            notes = stats.notes,
            tags = stats.tags,
            links = stats.links,
            unresolved = stats.unresolved_links,
            "vault indexed"
        );
    }
    Ok(vault)
}

/// Names of the tools the server exposes, for the `serve` startup banner.
fn tool_names() -> Vec<String> {
    ObsidianServer::tool_specs()
        .into_iter()
        .map(tool_name)
        .collect()
}

/// Run as a stdio MCP server for external hosts.
async fn run_serve(cli: &Cli) -> Result<()> {
    let vault = Arc::new(load_vault(cli)?);
    // The banner goes to stderr: stdout is the JSON-RPC channel and must stay
    // free of anything that is not an MCP frame.
    ui::print_serve_banner(
        &mut ui::stderr_console(),
        &vault,
        &tool_names(),
        "stdio (stdout = JSON-RPC)",
    )?;
    tracing::info!(
        vault = %vault.root().display(),
        "serving MCP over stdio (stdout is the JSON-RPC channel)"
    );

    let service = ObsidianServer::new(vault)
        .serve(rmcp::transport::io::stdio())
        .await
        .context("failed to start MCP server on stdio")?;
    service.waiting().await.context("MCP server stopped")?;
    Ok(())
}

/// Run as a Streamable-HTTP MCP server for external hosts.
///
/// The vault is still scanned exactly once: `StreamableHttpService::new` is
/// called once and its factory hands every session an `ObsidianServer` over the
/// same `Arc<Vault>`, rather than re-indexing per connection.
async fn run_serve_http(cli: &Cli, address: &str, allow_host: &[String]) -> Result<()> {
    let vault = Arc::new(load_vault(cli)?);
    let transport = format!("streamable HTTP · http://{address}/mcp");
    ui::print_serve_banner(&mut ui::stderr_console(), &vault, &tool_names(), &transport)?;
    tracing::info!(
        vault = %vault.root().display(),
        %address,
        "serving MCP over streamable HTTP"
    );

    let service = StreamableHttpService::new(
        move || Ok(ObsidianServer::new(vault.clone())),
        Arc::new(LocalSessionManager::default()),
        http_config(allow_host),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind {address}"))?;
    tracing::info!(%address, "listening for MCP requests");
    axum::serve(listener, router)
        .await
        .context("MCP HTTP server stopped")?;
    Ok(())
}

/// Build the HTTP transport configuration.
///
/// `rmcp`'s default accepts only loopback `Host` headers (its DNS-rebinding
/// guard). `--allow-host` adds public hostnames on top of that default rather
/// than replacing it, so `--allow-host vault.example.com` still answers on
/// `127.0.0.1`, and an empty list keeps the server unreachable from a browser
/// pointed at any other name.
fn http_config(allow_host: &[String]) -> StreamableHttpServerConfig {
    let config = StreamableHttpServerConfig::default();
    if allow_host.is_empty() {
        return config;
    }
    let mut hosts = config.allowed_hosts.clone();
    hosts.extend(allow_host.iter().cloned());
    config.with_allowed_hosts(hosts)
}

/// Name of a tool specification.
fn tool_name(spec: mcp_obsidian::ollama_client::ToolSpec) -> String {
    spec.name
}

/// Verify Ollama reachability, model presence and vault indexing.
async fn run_doctor(cli: &Cli) -> Result<()> {
    let client = OllamaClient::new(&cli.ollama_url, cli.api, &cli.model)?;
    let mut console = ui::stdout_console();

    ui::print_rule(&mut console, "configuration")?;
    ui::print_row(
        &mut console,
        "vault",
        &cli.vault.display().to_string(),
        theme::path(),
    )?;
    ui::print_row(&mut console, "model", client.model(), theme::value())?;
    ui::print_row(&mut console, "api", client.api().as_str(), theme::value())?;
    ui::print_row(&mut console, "base url", client.base_url(), theme::path())?;
    ui::print_row(&mut console, "chat url", &client.chat_url(), theme::path())?;

    ui::print_rule(&mut console, "ollama")?;
    match client.version().await {
        Ok(version) => ui::print_row(&mut console, "version", &version, theme::success())?,
        Err(error) => {
            ui::print_row(&mut console, "version", &format!("UNREACHABLE ({error})"), theme::error())?;
            ui::print_hint(&mut console, "start it with `ollama serve`")?;
        }
    }

    let mut model_ok = false;
    match client.models().await {
        Ok(models) => {
            model_ok = OllamaClient::model_available(&models, client.model());
            ui::print_row(
                &mut console,
                "models",
                &format!("{} found", models.len()),
                theme::value(),
            )?;
            ui::print_row(
                &mut console,
                "model check",
                &format!(
                    "{} is {}",
                    client.model(),
                    if model_ok { "available" } else { "MISSING" }
                ),
                if model_ok { theme::success() } else { theme::error() },
            )?;
            if !model_ok {
                let sample: Vec<&str> = models.iter().take(5).map(String::as_str).collect();
                ui::print_hint(
                    &mut console,
                    &format!("`ollama pull {}` (installed: {})", client.model(), sample.join(", ")),
                )?;
            }
        }
        Err(error) => ui::print_row(
            &mut console,
            "models",
            &format!("UNREACHABLE ({error})"),
            theme::error(),
        )?,
    }

    ui::print_rule(&mut console, "vault")?;
    match Vault::load(&cli.vault) {
        Ok(vault) => {
            ui::print_vault_stats(
                &mut console,
                vault.stats(),
                &vault.root().display().to_string(),
            )?;
            let unresolved = vault.unresolved_links();
            if !unresolved.is_empty() {
                let sample: Vec<&str> = unresolved.iter().take(5).map(String::as_str).collect();
                ui::print_hint(
                    &mut console,
                    &format!(
                        "{} unresolved link(s), e.g. {}",
                        unresolved.len(),
                        sample.join(", ")
                    ),
                )?;
            }
        }
        Err(_) => ui::print_row(
            &mut console,
            "root",
            "FAILED (vault path could not be indexed)",
            theme::error(),
        )?,
    }

    ui::print_rule(&mut console, "status")?;
    if model_ok {
        ui::print_line(&mut console, "ready", theme::success())?;
        ui::print_hint(
            &mut console,
            "next: mcp-obsidian --vault <path> ask \"what do my notes say about <topic>?\"",
        )?;
    } else {
        ui::print_line(&mut console, "not ready", theme::error())?;
        ui::print_hint(&mut console, "fix the ollama model check above, then re-run doctor")?;
    }
    Ok(())
}

/// Logging goes to stderr only, so stdout stays a clean MCP transport.
fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("mcp_obsidian={level},rmcp=warn")));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}
