//! Human-in-the-loop approval for write tools (rule 3). The agent loop calls an
//! `Approver` before any write executes; there is no path that skips it.
//!
//! Production always uses `TerminalApprover` (reads a real yes/no from the tty).
//! The auto-answering `ScriptedApprover` used by tests is `#[cfg(test)]` only, so
//! it does not exist in a release build — nothing can silently auto-approve.

use std::io::{self, Write};

pub trait Approver {
    /// Show `preview`, return true only on an explicit yes.
    fn confirm(&self, preview: &str) -> io::Result<bool>;
}

/// The only approver compiled into the shipping binary.
pub struct TerminalApprover;

impl Approver for TerminalApprover {
    fn confirm(&self, preview: &str) -> io::Result<bool> {
        println!("\n\x1b[1m● Proposed change — requires your approval\x1b[0m");
        println!("{preview}");
        print!("\nApply this change? [y/N]: ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES"))
    }
}

/// Test-only scripted approver. Gated behind `#[cfg(test)]` so it is impossible
/// to enable in a release build.
#[cfg(test)]
pub struct ScriptedApprover {
    answers: std::sync::Mutex<std::collections::VecDeque<bool>>,
}

#[cfg(test)]
impl ScriptedApprover {
    pub fn new(answers: Vec<bool>) -> Self {
        ScriptedApprover {
            answers: std::sync::Mutex::new(answers.into_iter().collect()),
        }
    }
}

#[cfg(test)]
impl Approver for ScriptedApprover {
    fn confirm(&self, _preview: &str) -> io::Result<bool> {
        // Default to NO if the script runs out — fail closed.
        Ok(self.answers.lock().unwrap().pop_front().unwrap_or(false))
    }
}
