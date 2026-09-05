//! VERIFY — confirm an outcome by re-sampling, never by assumption.
//!
//! The rule this stage exists to enforce: **an action is not "successful"
//! because it was dispatched.** After a recovery action is *applied*, the
//! system waits, takes a fresh reading, and compares. Only a real improvement
//! counts as recovery; anything else escalates.
//!
//! In the current dry-run world nothing is ever applied, so `verify` returns
//! [`VerifyOutcome::NotApplied`] without sampling — it will not claim a recovery
//! that never happened. The comparison logic is here and unit-tested so it is
//! ready the moment a real executor exists.

use std::io;

use crate::model::Severity;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VerifyOutcome {
    /// Severity dropped after the action — the problem eased.
    Recovered,
    /// Severity held or worsened — hand off / escalate.
    Escalated,
    /// Could not take a fresh reading.
    Inconclusive,
    /// No action was applied (e.g. dry-run) — nothing to verify.
    NotApplied,
}

impl VerifyOutcome {
    pub fn label(self) -> &'static str {
        match self {
            VerifyOutcome::Recovered => "RECOVERED",
            VerifyOutcome::Escalated => "ESCALATED",
            VerifyOutcome::Inconclusive => "INCONCLUSIVE",
            VerifyOutcome::NotApplied => "NOT-APPLIED",
        }
    }
}

impl std::fmt::Display for VerifyOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Compare severity before an applied action with the severity after.
pub fn assess(before: Severity, after: Severity) -> VerifyOutcome {
    if after < before {
        VerifyOutcome::Recovered
    } else {
        VerifyOutcome::Escalated
    }
}

/// Verify an outcome. `resample` is called **only** when `applied` is true, so
/// dry-run/no-op paths never even take a reading — they return `NotApplied`.
pub fn verify<F>(before: Severity, applied: bool, resample: F) -> VerifyOutcome
where
    F: FnOnce() -> io::Result<Severity>,
{
    if !applied {
        return VerifyOutcome::NotApplied;
    }
    match resample() {
        Ok(after) => assess(before, after),
        Err(_) => VerifyOutcome::Inconclusive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_drop_is_recovery() {
        assert_eq!(assess(Severity::Critical, Severity::Ok), VerifyOutcome::Recovered);
    }

    #[test]
    fn no_improvement_escalates() {
        assert_eq!(assess(Severity::Warning, Severity::Warning), VerifyOutcome::Escalated);
        assert_eq!(assess(Severity::Warning, Severity::Critical), VerifyOutcome::Escalated);
    }

    #[test]
    fn dry_run_never_claims_recovery() {
        // applied = false → resample must not even run.
        let out = verify(Severity::Critical, false, || panic!("must not sample"));
        assert_eq!(out, VerifyOutcome::NotApplied);
    }

    #[test]
    fn resample_failure_is_inconclusive() {
        let out = verify(Severity::Critical, true, || {
            Err(io::Error::new(io::ErrorKind::Other, "no reading"))
        });
        assert_eq!(out, VerifyOutcome::Inconclusive);
    }

    #[test]
    fn applied_and_improved_is_recovered() {
        let out = verify(Severity::Critical, true, || Ok(Severity::Ok));
        assert_eq!(out, VerifyOutcome::Recovered);
    }
}
