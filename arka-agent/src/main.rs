//! arka-agent binary — thin shell over the `arka_agent` library.

use clap::Parser;

use arka_agent::backend::dbus::DbusBackend;
use arka_agent::cli::{Cli, Command, LogCmd};
use arka_agent::config::Config;
use arka_agent::ollama::OllamaClient;
use arka_agent::{agent, audit};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load(cli.config.as_deref())?;

    match cli.command {
        Command::Ask { prompt } => {
            let llm = OllamaClient::new(&cfg);
            let backend = DbusBackend::new();
            let outcome = agent::run(&llm, &backend, &cfg, &prompt).await?;
            match outcome.final_answer {
                Some(answer) => println!("{answer}"),
                None => println!(
                    "(no final answer — stopped after {} step(s))",
                    outcome.steps_used
                ),
            }
        }
        Command::Log { cmd } => match cmd {
            LogCmd::Verify => println!("{}", audit::verify(&cfg.audit_path)?),
        },
    }
    Ok(())
}
