//! [`OllamaClient`]: endpoint construction, reachability probes and one chat
//! round. The request/response dialects live in [`super::wire`] and
//! [`super::parse`].

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::parse::{parse_native_response, parse_openai_response};
use super::types::{
    ApiFlavor, AssistantTurn, ChatMessage, DEFAULT_TIMEOUT_SECS, ToolSpec,
};
use super::wire::{normalize_root_url, request_body};

/// Minimal HTTP client for a local Ollama server.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    /// Server root, e.g. `http://localhost:11434` (never includes `/v1`).
    root_url: String,
    api: ApiFlavor,
    model: String,
}

impl OllamaClient {
    /// Build a client. `base_url` may include the `/v1` suffix or not.
    pub fn new(
        base_url: impl Into<String>,
        api: ApiFlavor,
        model: impl Into<String>,
    ) -> Result<Self> {
        let root_url = normalize_root_url(&base_url.into());
        if root_url.is_empty() {
            bail!("ollama base url must not be empty");
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            http,
            root_url,
            api,
            model: model.into(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.root_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn api(&self) -> ApiFlavor {
        self.api
    }

    // -- Endpoints ----------------------------------------------------------

    /// Ollama native chat endpoint.
    pub fn native_url(&self) -> String {
        format!("{}/api/chat", self.root_url)
    }

    /// OpenAI compatible chat endpoint.
    pub fn openai_url(&self) -> String {
        format!("{}/v1/chat/completions", self.root_url)
    }

    /// URL used for chat completions.
    pub fn chat_url(&self) -> String {
        match self.api {
            ApiFlavor::Native => self.native_url(),
            ApiFlavor::Openai => self.openai_url(),
        }
    }

    /// URL used for the reachability probe.
    pub fn version_url(&self) -> String {
        format!("{}/api/version", self.root_url)
    }

    // -- Reachability -------------------------------------------------------

    /// Report the Ollama server version.
    pub async fn version(&self) -> Result<String> {
        let url = self.version_url();
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("cannot reach Ollama at {url}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("Ollama returned HTTP {status} for {url}");
        }
        let body: Value = response
            .json()
            .await
            .context("Ollama version response was not valid JSON")?;
        Ok(body
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string())
    }

    /// List locally available model names, trying `/api/tags` then `/v1/models`.
    pub async fn models(&self) -> Result<Vec<String>> {
        if let Some(names) = self.native_model_names().await? {
            return Ok(names);
        }
        self.openai_model_names().await
    }

    /// `true` when `model` is present locally, tolerating an implicit `:latest`.
    pub fn model_available(models: &[String], model: &str) -> bool {
        let wanted = model.trim();
        if wanted.is_empty() {
            return false;
        }
        models.iter().any(|candidate| {
            candidate == wanted
                || candidate == &format!("{wanted}:latest")
                || candidate
                    .strip_suffix(":latest")
                    .map(|base| base == wanted)
                    .unwrap_or(false)
        })
    }

    /// `/api/tags`. `Ok(None)` means "endpoint gave nothing useful", which lets
    /// the caller fall back to the OpenAI compatible listing.
    async fn native_model_names(&self) -> Result<Option<Vec<String>>> {
        let url = format!("{}/api/tags", self.root_url);
        let Ok(response) = self.http.get(&url).send().await else {
            return Ok(None);
        };
        if !response.status().is_success() {
            return Ok(None);
        }
        let body: Value = response
            .json()
            .await
            .context("Ollama /api/tags response was not valid JSON")?;
        let Some(entries) = body.get("models").and_then(Value::as_array) else {
            return Ok(None);
        };
        let names = entries.iter().filter_map(|entry| {
            entry
                .get("model")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        Ok(Some(sorted_unique(names)))
    }

    /// `/v1/models`, the fallback for servers that only speak the OpenAI shape.
    async fn openai_model_names(&self) -> Result<Vec<String>> {
        let native = format!("{}/api/tags", self.root_url);
        let openai = format!("{}/v1/models", self.root_url);
        let response = self
            .http
            .get(&openai)
            .send()
            .await
            .with_context(|| format!("cannot list models at {native} or {openai}"))?;
        let body: Value = response
            .json()
            .await
            .context("model list response was not valid JSON")?;
        let names = body
            .get("data")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        Ok(sorted_unique(names))
    }

    // -- Chat ---------------------------------------------------------------

    /// Send the conversation (and optional tool definitions) to the model.
    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Result<AssistantTurn> {
        let request = request_body(self.api, &self.model, messages, tools);
        let url = self.chat_url();
        let response = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("cannot reach Ollama at {url}"))?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .with_context(|| format!("Ollama returned a non-JSON response from {url} (HTTP {status})"))?;
        if !status.is_success() {
            bail!("Ollama error (HTTP {status}): {}", error_detail(&body));
        }

        match self.api {
            ApiFlavor::Native => parse_native_response(&body),
            ApiFlavor::Openai => parse_openai_response(&body),
        }
    }
}

/// Sort and de-duplicate model names, preserving the deterministic order the
/// `doctor` command prints.
fn sorted_unique(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = names.into_iter().collect();
    names.sort();
    names.dedup();
    names
}

/// Prefer Ollama's `error` field, fall back to the whole body.
fn error_detail(body: &Value) -> String {
    body.get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| body.to_string())
}
