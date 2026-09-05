//! RECOVER — propose an action, gate it, and (only ever) *log* what it would do.
//!
//! This is the one stage that could touch the machine, so it is the most
//! conservative. The pipeline is fixed and there is no path from model text to
//! a process:
//!
//! ```text
//! intent → ACTION REGISTRY → POLICY ENGINE → approve/require-approval/deny
//!        → executor (dry-run: logs a fixed argv, never spawns)
//! ```
//!
//! Invariants:
//! - **Ships disabled and dry-run** ([`RecoveryConfig::default`]).
//! - **Fixed action registry.** An [`Intent`] maps to a hardcoded argv *array*;
//!   there is no shell, and no model string is ever interpolated into a command.
//! - **No real executor exists yet.** Even with `dry_run = false`, this crate
//!   refuses to spawn anything — it returns [`ExecOutcome::NotImplemented`].
//! - **Critical risk is denied outright**; disabled ⇒ everything requires human
//!   approval; only when explicitly enabled can low-risk actions auto-approve.

use crate::explain::{Explanation, Intent, Risk};

/// Recovery configuration. The defaults are the safe ones.
#[derive(Clone, Copy)]
pub struct RecoveryConfig {
    /// Master switch. When false, nothing is ever auto-approved.
    pub enabled: bool,
    /// When true, the executor only logs; it never spawns a process.
    pub dry_run: bool,
    /// Highest risk that may auto-approve when `enabled` (kept at Low).
    pub auto_max_risk: Risk,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        RecoveryConfig {
            enabled: false,
            dry_run: true,
            auto_max_risk: Risk::Low,
        }
    }
}

/// The policy engine's verdict for a proposed action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    /// The intent maps to no action (e.g. NONE / INVESTIGATE_MANUALLY).
    NoAction,
    /// Allowed to run automatically (low risk, recovery enabled).
    AutoApprove,
    /// Must be confirmed by a human first.
    RequireApproval,
    /// Refused outright (critical risk).
    Deny,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Decision::NoAction => "no-action",
            Decision::AutoApprove => "auto-approve",
            Decision::RequireApproval => "require-approval",
            Decision::Deny => "deny",
        }
    }
}

/// A concrete, predefined action. `argv` is exactly what *would* be run — a
/// fixed array, never a shell string, and it is never executed by this crate.
#[derive(Clone)]
pub struct Action {
    pub intent: Intent,
    pub description: &'static str,
    pub argv: &'static [&'static str],
    pub risk: Risk,
}

/// The action registry: the *only* commands that can ever be associated with an
/// intent. Risk comes from [`Intent::risk`] so it cannot drift.
pub fn action_for(intent: Intent) -> Option<Action> {
    let (description, argv): (&'static str, &'static [&'static str]) = match intent {
        Intent::FreeReclaimableMemory => ("drop reclaimable caches", &["sysctl", "vm.drop_caches=1"]),
        Intent::RestartNetworkService => {
            ("restart the network service", &["systemctl", "restart", "NetworkManager"])
        }
        Intent::ReduceThermalLoad => ("start the thermal daemon", &["systemctl", "start", "thermald"]),
        Intent::None | Intent::InvestigateManually => return None,
    };
    Some(Action {
        intent,
        description,
        argv,
        risk: intent.risk(),
    })
}

/// The policy engine. Pure function of config + action risk.
pub fn decide(cfg: &RecoveryConfig, action: &Action) -> Decision {
    if action.risk == Risk::Critical {
        return Decision::Deny; // never, regardless of config
    }
    if !cfg.enabled {
        return Decision::RequireApproval; // disabled ⇒ never auto
    }
    if action.risk <= cfg.auto_max_risk {
        Decision::AutoApprove
    } else {
        Decision::RequireApproval
    }
}

/// What the executor did. Only ever `DryRun` or `NotImplemented` today.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ExecOutcome {
    /// Logged the fixed argv; nothing was spawned.
    DryRun { would_run: String },
    /// Real execution requested but deliberately not built — refused.
    NotImplemented,
    /// Reserved for a future real executor. Never produced today.
    Applied,
}

/// The "executor". It joins the fixed argv for logging and returns — it never
/// constructs a `Command`, never uses a shell, never interpolates model text.
pub fn execute(action: &Action, cfg: &RecoveryConfig) -> ExecOutcome {
    if cfg.dry_run {
        ExecOutcome::DryRun {
            would_run: action.argv.join(" "),
        }
    } else {
        // A real executor does not exist yet; refuse rather than improvise.
        ExecOutcome::NotImplemented
    }
}

/// The full outcome of handling one incident through RECOVER.
pub struct RecoveryReport {
    pub intent: Intent,
    pub action: Option<Action>,
    pub decision: Decision,
    pub exec: Option<ExecOutcome>,
}

impl RecoveryReport {
    /// Whether a real action actually reached the system (always false today).
    pub fn applied(&self) -> bool {
        matches!(self.exec, Some(ExecOutcome::Applied))
    }
}

/// Plan recovery for an explanation: registry → policy → (dry-run) executor.
pub fn plan(explanation: &Explanation, cfg: &RecoveryConfig) -> RecoveryReport {
    let intent = explanation.intent;
    let action = action_for(intent);

    let (decision, exec) = match &action {
        None => (Decision::NoAction, None),
        Some(a) => {
            let decision = decide(cfg, a);
            let exec = if decision == Decision::AutoApprove {
                Some(execute(a, cfg))
            } else {
                None
            };
            (decision, exec)
        }
    };

    RecoveryReport {
        intent,
        action,
        decision,
        exec,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explain::{Explanation, Source};

    fn expl(intent: Intent) -> Explanation {
        Explanation {
            diagnosis: "d".into(),
            impact: "i".into(),
            evidence: vec![],
            intent,
            source: Source::Fallback,
        }
    }

    #[test]
    fn registry_maps_known_intents_only() {
        assert!(action_for(Intent::FreeReclaimableMemory).is_some());
        assert!(action_for(Intent::None).is_none());
        assert!(action_for(Intent::InvestigateManually).is_none());
    }

    #[test]
    fn critical_risk_is_always_denied() {
        let a = Action {
            intent: Intent::RestartNetworkService,
            description: "x",
            argv: &["true"],
            risk: Risk::Critical,
        };
        let cfg = RecoveryConfig { enabled: true, dry_run: true, auto_max_risk: Risk::High };
        assert_eq!(decide(&cfg, &a), Decision::Deny);
    }

    #[test]
    fn disabled_requires_approval_even_for_low_risk() {
        let a = action_for(Intent::FreeReclaimableMemory).unwrap();
        assert_eq!(a.risk, Risk::Low);
        assert_eq!(decide(&RecoveryConfig::default(), &a), Decision::RequireApproval);
    }

    #[test]
    fn enabled_auto_approves_low_but_not_medium() {
        let cfg = RecoveryConfig { enabled: true, dry_run: true, auto_max_risk: Risk::Low };
        let low = action_for(Intent::FreeReclaimableMemory).unwrap();
        let med = action_for(Intent::RestartNetworkService).unwrap();
        assert_eq!(decide(&cfg, &low), Decision::AutoApprove);
        assert_eq!(decide(&cfg, &med), Decision::RequireApproval);
    }

    #[test]
    fn auto_approved_dry_run_logs_but_never_applies() {
        let cfg = RecoveryConfig { enabled: true, dry_run: true, auto_max_risk: Risk::Low };
        let rep = plan(&expl(Intent::FreeReclaimableMemory), &cfg);
        assert_eq!(rep.decision, Decision::AutoApprove);
        match rep.exec {
            Some(ExecOutcome::DryRun { ref would_run }) => {
                assert!(would_run.contains("vm.drop_caches"));
            }
            other => panic!("expected dry-run, got {other:?}"),
        }
        assert!(!rep.applied()); // nothing ever reaches the system
    }

    #[test]
    fn real_execution_is_refused() {
        let cfg = RecoveryConfig { enabled: true, dry_run: false, auto_max_risk: Risk::Low };
        let a = action_for(Intent::FreeReclaimableMemory).unwrap();
        assert_eq!(execute(&a, &cfg), ExecOutcome::NotImplemented);
    }
}
