//! Configuration: local-LLM endpoint, models, audit path, and safety knobs.
//! Every field has a secure-by-default value, so the agent works with no file.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Ollama base URL. Localhost only by policy (rule 2).
    pub ollama_url: String,
    /// The local model. There is deliberately no fallback: qwen2.5:3b failed
    /// 3/20 live eval cases by looping on its JSON output (2026-09-25), so a
    /// machine that can't run this model gets "unavailable", not a weaker one.
    pub model: String,
    /// Where the JSONL audit log lives.
    pub audit_path: String,
    /// If true, write tools never actually change the system (still audited).
    pub dry_run: bool,
    /// Advisory step cap; the loop also hard-caps at 5 (rule 6).
    pub max_steps: usize,
    /// systemd units the `restart_service` tool is allowed to touch.
    pub allowed_units: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ollama_url: "http://127.0.0.1:11434".into(),
            model: "qwen2.5:7b-instruct".into(),
            audit_path: default_audit_path(),
            dry_run: false,
            max_steps: 5,
            allowed_units: vec!["NetworkManager".into()],
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn default_audit_path() -> String {
    if let Some(mut p) = home() {
        p.push(".local/state/arka-agent/audit.jsonl");
        return p.to_string_lossy().into_owned();
    }
    "arka-agent-audit.jsonl".into()
}

impl Config {
    pub fn load(path: Option<&str>) -> anyhow::Result<Config> {
        let candidate = path.map(PathBuf::from).or_else(|| {
            home().map(|mut p| {
                p.push(".config/arka-agent/config.toml");
                p
            })
        });
        if let Some(p) = candidate {
            if p.exists() {
                let s = std::fs::read_to_string(&p)?;
                let cfg: Config = toml::from_str(&s)?;
                return Ok(cfg);
            }
        }
        Ok(Config::default())
    }
}
