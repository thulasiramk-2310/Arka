//! Local LLM client (Ollama over localhost HTTP) behind an `Llm` trait so tests
//! can swap in a mock with no GPU (rule 2, Phase 4).
//!
//! Tool-calling is implemented at the agent layer as a strict JSON protocol
//! (see `schema.rs`), not via Ollama's function-calling — this keeps the
//! fail-closed parsing in our control and makes the mock trivial. We do set
//! `format: "json"` so the model is nudged to emit a single JSON object.

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
/// so the native async-fn-in-trait is fine here.
#[allow(async_fn_in_trait)]
pub trait Llm {
    async fn complete(&self, messages: &[ChatMsg]) -> anyhow::Result<String>;
}

pub struct OllamaClient {
    url: String,
    model: String,
    fallback: String,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new(cfg: &Config) -> Self {
        OllamaClient {
            url: cfg.ollama_url.clone(),
            model: cfg.model.clone(),
            fallback: cfg.fallback_model.clone(),
            http: reqwest::Client::new(),
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
            options: Opts { temperature: 0.0 },
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
        match self.call(&self.model, messages).await {
            Ok(s) => Ok(s),
            Err(e) => {
                eprintln!(
                    "arka-agent: primary model '{}' failed ({e}); trying fallback '{}'",
                    self.model, self.fallback
                );
                self.call(&self.fallback, messages).await
            }
        }
    }
}
