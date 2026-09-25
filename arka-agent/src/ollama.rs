//! Local LLM client (Ollama over localhost HTTP) behind an `Llm` trait, plus a
//! scripted `MockLlm` so tests and the eval run with no GPU (rule 2).
//!
//! Tool-calling is our own strict JSON protocol (see `schema.rs`), not Ollama's
//! function-calling — keeps fail-closed parsing in our control. We set
//! `format: "json"` to nudge the model toward a single JSON object.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone, Serialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

impl ChatMsg {
    pub fn system(c: impl Into<String>) -> Self {
        ChatMsg {
            role: "system".into(),
            content: c.into(),
        }
    }
    pub fn user(c: impl Into<String>) -> Self {
        ChatMsg {
            role: "user".into(),
            content: c.into(),
        }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        ChatMsg {
            role: "assistant".into(),
            content: c.into(),
        }
    }
}

/// The LLM abstraction. Static dispatch only (agent is generic over `L: Llm`),
/// so native async-fn-in-trait is fine here.
#[allow(async_fn_in_trait)]
pub trait Llm {
    async fn complete(&self, messages: &[ChatMsg]) -> anyhow::Result<String>;
}

// ── real client ──────────────────────────────────────────────────────────────

/// Hard cap on one model reply. A valid step or final answer is a few hundred
/// tokens at most; qwen2.5:3b was seen looping to ~190 KB of unclosed JSON, which
/// took minutes per call. A capped runaway comes back truncated, fails
/// `parse_step`, and is rejected like any other bad output (fail closed).
const MAX_REPLY_TOKENS: u32 = 512;

/// Wall-clock backstop per request, so a stuck runtime can't hang the agent.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

pub struct OllamaClient {
    url: String,
    model: String,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new(cfg: &Config) -> Self {
        OllamaClient {
            url: cfg.ollama_url.clone(),
            model: cfg.model.clone(),
            http: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("static reqwest config"),
        }
    }

    async fn call(&self, model: &str, messages: &[ChatMsg]) -> anyhow::Result<String> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: &'a [ChatMsg],
            stream: bool,
            format: &'a str,
            options: Opts,
        }
        #[derive(Serialize)]
        struct Opts {
            temperature: f32,
            num_predict: u32,
        }
        #[derive(Deserialize)]
        struct Resp {
            message: RespMsg,
        }
        #[derive(Deserialize)]
        struct RespMsg {
            content: String,
        }

        let req = Req {
            model,
            messages,
            stream: false,
            format: "json",
            options: Opts {
                temperature: 0.0,
                num_predict: MAX_REPLY_TOKENS,
            },
        };
        let resp = self
            .http
            .post(format!("{}/api/chat", self.url))
            .json(&req)
            .send()
            .await?
            .error_for_status()?;
        let body: Resp = resp.json().await?;
        Ok(body.message.content)
    }
}

impl Llm for OllamaClient {
    async fn complete(&self, messages: &[ChatMsg]) -> anyhow::Result<String> {
        // No fallback model on purpose (see `Config::model`): if this model
        // can't answer, say so plainly instead of quietly using a weaker one.
        self.call(&self.model, messages).await.map_err(|e| {
            anyhow::anyhow!(
                "local model '{}' is not available ({e}). arka-agent needs it running \
                 in Ollama on this machine and does not fall back to a smaller model.",
                self.model
            )
        })
    }
}

// ── scripted mock (tests + mock eval; no GPU) ────────────────────────────────

/// Returns a fixed sequence of replies, one per `complete` call. When the
/// script is exhausted it errors — a test that loops too far fails loudly.
pub struct MockLlm {
    replies: Mutex<VecDeque<String>>,
    /// Every message batch the agent sent, for assertions (e.g. injection tests).
    pub seen: Mutex<Vec<Vec<ChatMsg>>>,
}

impl MockLlm {
    pub fn new(replies: Vec<impl Into<String>>) -> Self {
        MockLlm {
            replies: Mutex::new(replies.into_iter().map(Into::into).collect()),
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl Llm for MockLlm {
    async fn complete(&self, messages: &[ChatMsg]) -> anyhow::Result<String> {
        self.seen.lock().unwrap().push(messages.to_vec());
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("MockLlm script exhausted"))
    }
}
