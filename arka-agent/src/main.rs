//! arka-agent — on-device AI agent for ArkaOS.
//!
//! The LLM is local (Ollama) and can only *propose* tool calls. Read tools run
//! automatically; write tools require terminal approval. Every call is audited
//! in a hash-chained JSONL log. The hard rules are listed in the crate docs.

mod agent;
mod approval;
mod audit;
mod cli;
mod config;
mod ollama;
mod schema;
mod tools;

use clap::Parser;
use cli::{Cli, Command, LogCmd};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = config::Config::load(cli.config.as_deref())?;

    match cli.command {
        Command::Ask { prompt } => {
            let llm = ollama::OllamaClient::new(&cfg);
            agent::run(&llm, &cfg, &prompt).await?;
        }
        Command::Log { cmd } => match cmd {
            LogCmd::Verify => {
                let report = audit::verify(&cfg.audit_path)?;
                println!("{report}");
            }
        },
    }
    Ok(())
}
