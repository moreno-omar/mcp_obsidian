//! `mcp_obsidian` — CLI host and MCP server for an Obsidian markdown vault.
//!
//! Two ways to use the same retrieval core:
//!
//! * `ask` / `chat`  — local host loop: prompt → Ollama → vault tools → answer.
//! * `serve`         — expose the tools to external MCP hosts over stdio.
//!
//! Logging always goes to **stderr**; stdout is reserved for CLI output and, in
//! `serve` mode, for the JSON-RPC MCP stream.

mod host;
mod mcp_server;
mod ollama_client;
mod vault;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

use crate::host::Host;
use crate::mcp_server::{
    GetNoteRequest, LinkedNotesRequest, ObsidianServer, SearchByTagRequest, SearchByTermRequest,
    run_get_linked_notes, run_get_note, run_search_by_tag, run_search_by_term,
};
use crate::ollama_client::{ApiFlavor, DEFAULT_BASE_URL, DEFAULT_MODEL, OllamaClient};
use crate::vault::{DEFAULT_BODY_CHARS, DEFAULT_LIMIT, Vault};

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
    name = "mcp_obsidian",
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

        /// Maximum model rounds before giving up.
        #[arg(long, default_value_t = host::DEFAULT_MAX_ROUNDS)]
        max_rounds: usize,

        /// Print every tool call the model made to stderr.
        #[arg(long)]
        trace: bool,
    },

    /// Interactive prompt loop with vault tools enabled.
    Chat {
        /// Skip tool calling and just chat.
        #[arg(long)]
        no_tools: bool,

        /// Maximum model rounds per prompt.
        #[arg(long, default_value_t = host::DEFAULT_MAX_ROUNDS)]
        max_rounds: usize,
    },

    /// Serve the retrieval tools to MCP hosts over stdio.
    Serve,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match &cli.command {
        Command::Serve => run_serve(&cli).await,
        Command::Doctor => run_doctor(&cli).await,
        Command::Index { limit } => {
            let vault = load_vault(&cli)?;
            println!("{}", vault.render_index(*limit));
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
            let vault = load_vault(&cli)?;
            let output = run_search(
                &vault, *kind, query, *match_all, *limit, *depth, direction, *max_chars,
            );
            println!("{output}");
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
            max_rounds,
            trace,
        } => {
            let host = build_host(&cli, !*no_tools, *max_rounds)?;
            let outcome = host.ask(&prompt.join(" ")).await?;
            if *trace {
                for (tool, arguments) in &outcome.trace {
                    eprintln!("tool: {tool} {arguments}");
                }
                eprintln!(
                    "rounds: {} | tool calls: {}",
                    outcome.rounds, outcome.tool_calls
                );
            }
            println!("{}", outcome.answer.trim());
            Ok(())
        }
        Command::Chat {
            no_tools,
            max_rounds,
        } => {
            let host = build_host(&cli, !*no_tools, *max_rounds)?;
            host.chat_loop().await
        }
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
fn build_host(cli: &Cli, use_tools: bool, max_rounds: usize) -> Result<Host> {
    let vault = Arc::new(load_vault(cli)?);
    let client = OllamaClient::new(&cli.ollama_url, cli.api, &cli.model)?;
    let server = Arc::new(ObsidianServer::new(vault));
    let host = Host::new(client, server).with_max_rounds(max_rounds);
    Ok(if use_tools {
        host
    } else {
        host.without_tools()
    })
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

/// Run as a stdio MCP server for external hosts.
async fn run_serve(cli: &Cli) -> Result<()> {
    let vault = Arc::new(load_vault(cli)?);
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

/// Verify Ollama reachability, model presence and vault indexing.
async fn run_doctor(cli: &Cli) -> Result<()> {
    let client = OllamaClient::new(&cli.ollama_url, cli.api, &cli.model)?;
    println!("configuration");
    println!("  vault       : {}", cli.vault.display());
    println!("  model       : {}", client.model());
    println!("  api         : {}", client.api().as_str());
    println!("  base url    : {}", client.base_url());
    println!("  chat url    : {}", client.chat_url());

    println!("\nollama");
    match client.version().await {
        Ok(version) => println!(
            "  version     : {version} (reachable at {} /api/version)",
            client.base_url()
        ),
        Err(error) => {
            println!("  version     : UNREACHABLE ({error})");
            println!("  hint        : start it with `ollama serve`");
        }
    }

    let mut model_ok = false;
    match client.models().await {
        Ok(models) => {
            model_ok = OllamaClient::model_available(&models, client.model());
            println!("  models      : {} found", models.len());
            println!(
                "  model check : {} is {}",
                client.model(),
                if model_ok { "available" } else { "MISSING" }
            );
            if !model_ok {
                let sample: Vec<&str> = models.iter().take(5).map(String::as_str).collect();
                println!(
                    "  hint        : `ollama pull {}` (installed: {})",
                    client.model(),
                    sample.join(", ")
                );
            }
        }
        Err(error) => println!("  models      : UNREACHABLE ({error})"),
    }

    println!("\nvault");
    if let Ok(vault) = Vault::load(&cli.vault) {
        let stats = vault.stats();
        println!("  root        : {}", vault.root().display());
        println!(
            "  indexes     : {} notes, {} tags, {} headings, {} links ({} unresolved)",
            stats.notes, stats.tags, stats.headings, stats.links, stats.unresolved_links
        );
        let unresolved = vault.unresolved_links();
        if !unresolved.is_empty() {
            let sample: Vec<&str> = unresolved.iter().take(5).map(String::as_str).collect();
            println!(
                "  unresolved  : {} (e.g. {})",
                unresolved.len(),
                sample.join(", ")
            );
        }
    } else {
        println!("  root        : FAILED (vault path could not be indexed)");
    }

    println!("\nstatus: {}", if model_ok { "ready" } else { "not ready" });
    println!("next  : mcp_obsidian --vault <path> ask \"what do my notes say about <topic>?\"");
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
