//! Human-in-the-loop approval for write tools (rule 3). The agent loop calls an
//! `Approver` before any write executes; there is no path that skips it.
//!
//! Two gates:
//!   `confirm`       — an ordinary write: an explicit y/N.
//!   `confirm_typed` — a write that LOWERS protection (#4): a plain-words
//!                     consequence warning plus a TYPED phrase, so a weakening
//!                     change can never ride through on a reflexive "y".
//!
//! Production always uses `TerminalApprover` (reads a real tty). The
//! auto-answering `ScriptedApprover` used by tests is `#[cfg(test)]` only, so it
//! does not exist in a release build — nothing can silently auto-approve.

use std::io::{self, Write};

pub trait Approver {
    /// Show `preview`, return true only on an explicit yes.
    fn confirm(&self, preview: &str) -> io::Result<bool>;

    /// Protection-lowering write (#4): warn in plain words, then require the
    /// user to type `phrase` exactly. Return true only on an exact match.
    fn confirm_typed(&self, preview: &str, phrase: &str) -> io::Result<bool>;
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

    fn confirm_typed(&self, preview: &str, phrase: &str) -> io::Result<bool> {
        println!("\n\x1b[1;31m● This change LOWERS a privacy protection on this device.\x1b[0m");
        println!("{preview}");
        println!("\nIf you proceed, this system is less protected until you turn it back on.");
        print!("Type exactly \u{201c}{phrase}\u{201d} to proceed (anything else cancels): ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(line.trim() == phrase)
    }
}

/// Test-only scripted approver. Gated behind `#[cfg(test)]` so it is impossible
/// to enable in a release build.
#[cfg(test)]
pub struct ScriptedApprover {
    answers: std::sync::Mutex<std::collections::VecDeque<bool>>,
    typed_called: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl ScriptedApprover {
    pub fn new(answers: Vec<bool>) -> Self {
        ScriptedApprover {
            answers: std::sync::Mutex::new(answers.into_iter().collect()),
            typed_called: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// True if the typed (protection-lowering) path was taken — lets a test
    /// prove a weakening write was routed through `confirm_typed`, not `confirm`.
    pub fn typed_was_called(&self) -> bool {
        self.typed_called.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl Approver for ScriptedApprover {
    fn confirm(&self, _preview: &str) -> io::Result<bool> {
        // Default to NO if the script runs out — fail closed.
        Ok(self.answers.lock().unwrap().pop_front().unwrap_or(false))
    }

    fn confirm_typed(&self, _preview: &str, _phrase: &str) -> io::Result<bool> {
        self.typed_called
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(self.answers.lock().unwrap().pop_front().unwrap_or(false))
    }
}
