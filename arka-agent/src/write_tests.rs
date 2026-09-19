//! Phase 3 write-path tests. Unit tests (compiled under `cfg(test)`) so they can
//! use the test-only `ScriptedApprover`. No GPU, no daemon, no stdin.

use std::sync::atomic::Ordering;

use serde_json::json;

use crate::agent;
use crate::approval::ScriptedApprover;
use crate::backend::MockBackend;
use crate::config::Config;
use crate::ollama::MockLlm;

fn tmp_cfg(dry_run: bool) -> Config {
    let mut cfg = Config::default();
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!("arka-agent-wt-{nanos}.jsonl"));
    cfg.audit_path = p.to_string_lossy().into_owned();
    cfg.dry_run = dry_run;
    cfg
}

fn read_log(cfg: &Config) -> String {
    std::fs::read_to_string(&cfg.audit_path).unwrap_or_default()
}

#[tokio::test]
async fn write_requires_approval_and_denial_makes_no_change() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![r#"{"tool":"enforce_privacy","args":{}}"#]);
    let approver = ScriptedApprover::new(vec![false]); // say NO
    let cfg = tmp_cfg(false);

    let out = agent::run(&llm, &backend, &approver, &cfg, "re-enforce privacy")
        .await
        .unwrap();
    assert!(out.final_answer.is_none());
    assert_eq!(
        backend.writes_total(),
        0,
        "denied write must not reach the backend"
    );

    let log = read_log(&cfg);
    assert!(
        log.contains("\"decision\":\"denied\""),
        "denial must be logged"
    );
    assert!(crate::audit::verify(&cfg.audit_path)
        .unwrap()
        .contains("chain intact"));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn approved_enforce_calls_backend_once() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"enforce_privacy","args":{}}"#,
        r#"{"final":"Re-applied all privacy enforcers."}"#,
    ]);
    let approver = ScriptedApprover::new(vec![true]);
    let cfg = tmp_cfg(false);

    let out = agent::run(&llm, &backend, &approver, &cfg, "re-enforce privacy")
        .await
        .unwrap();
    assert_eq!(
        out.final_answer.as_deref(),
        Some("Re-applied all privacy enforcers.")
    );
    assert_eq!(
        backend.enforce_calls.load(Ordering::SeqCst),
        1,
        "approved write must hit the backend once"
    );

    assert!(read_log(&cfg).contains("\"decision\":\"approved\""));
    assert!(crate::audit::verify(&cfg.audit_path)
        .unwrap()
        .contains("chain intact"));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

/// The key Phase-3 guarantee: dry-run is approved, logged, and NEVER calls a
/// backend write method.
#[tokio::test]
async fn dry_run_never_calls_backend() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"enforce_privacy","args":{}}"#,
        r#"{"final":"Dry-run only."}"#,
    ]);
    let approver = ScriptedApprover::new(vec![true]); // approved, but dry-run
    let cfg = tmp_cfg(true);

    agent::run(&llm, &backend, &approver, &cfg, "re-enforce privacy")
        .await
        .unwrap();
    assert_eq!(
        backend.writes_total(),
        0,
        "dry-run must never call a backend write method"
    );

    let log = read_log(&cfg);
    assert!(
        log.contains("\"decision\":\"dry-run\""),
        "dry-run attempt must be logged"
    );
    assert!(log.contains("dry-run"), "result should mark it dry-run");
    assert!(crate::audit::verify(&cfg.audit_path)
        .unwrap()
        .contains("chain intact"));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn set_privacy_setting_reports_not_implemented_in_arkad() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"set_privacy_setting","args":{"setting":"mac","enabled":false}}"#,
        r#"{"final":"arkad has no setter yet."}"#,
    ]);
    let approver = ScriptedApprover::new(vec![true]);
    let cfg = tmp_cfg(false);

    agent::run(
        &llm,
        &backend,
        &approver,
        &cfg,
        "turn off MAC randomization",
    )
    .await
    .unwrap();
    // Backend was called (real, non-dry) and returned the honest error.
    assert_eq!(backend.setting_calls.load(Ordering::SeqCst), 1);
    assert!(read_log(&cfg).contains("not implemented in arkad"));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn bad_write_args_rejected_before_approval() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"set_privacy_setting","args":{"setting":"wifi","enabled":true}}"#, // invalid setting
        r#"{"final":"Can't do that."}"#,
    ]);
    // If approval were ever reached this would say yes; it must NOT be reached.
    let approver = ScriptedApprover::new(vec![true]);
    let cfg = tmp_cfg(false);

    let out = agent::run(&llm, &backend, &approver, &cfg, "set wifi setting")
        .await
        .unwrap();
    assert_eq!(out.final_answer.as_deref(), Some("Can't do that."));
    assert_eq!(
        backend.writes_total(),
        0,
        "invalid args must never reach the backend"
    );

    let log = read_log(&cfg);
    assert!(
        log.contains("\"decision\":\"rejected\""),
        "bad args must be logged as rejected"
    );
    assert!(
        !log.contains("\"decision\":\"approved\""),
        "must not have been approved"
    );
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn restart_service_rejects_unlisted_unit() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"restart_service","args":{"unit":"sshd"}}"#, // not in allow-list
        r#"{"final":"Not allowed."}"#,
    ]);
    let approver = ScriptedApprover::new(vec![true]);
    let cfg = tmp_cfg(false); // default allow-list = ["NetworkManager"]

    agent::run(&llm, &backend, &approver, &cfg, "restart sshd")
        .await
        .unwrap();
    assert_eq!(
        backend.restart_calls.load(Ordering::SeqCst),
        0,
        "unlisted unit must not be restarted"
    );
    assert!(read_log(&cfg).contains("\"decision\":\"rejected\""));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn log_verify_passes_after_mixed_writes() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"enforce_privacy","args":{}}"#,
        r#"{"final":"done"}"#,
    ]);
    let approver = ScriptedApprover::new(vec![true]);
    let cfg = tmp_cfg(false);
    let _ = json!({}); // keep serde_json import used

    agent::run(&llm, &backend, &approver, &cfg, "enforce")
        .await
        .unwrap();
    let report = crate::audit::verify(&cfg.audit_path).unwrap();
    assert!(report.contains("chain intact"), "{report}");
    let _ = std::fs::remove_file(&cfg.audit_path);
}
