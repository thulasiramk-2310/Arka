//! Phase 2 integration tests — mock LLM + mock backend, no GPU, no daemon.
//! Exercises the read path, fail-closed rules, and the audit chain end to end.

use arka_agent::agent;
use arka_agent::approval::TerminalApprover;
use arka_agent::backend::MockBackend;
use arka_agent::config::Config;
use arka_agent::ollama::MockLlm;

fn tmp_cfg() -> Config {
    let mut cfg = Config::default();
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!("arka-agent-it-{nanos}.jsonl"));
    cfg.audit_path = p.to_string_lossy().into_owned();
    cfg
}

#[tokio::test]
async fn ask_dns_uses_read_tool_then_answers() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"system_status","args":{}}"#,
        r#"{"final":"DNS-over-TLS is on (DoT active, Quad9)."}"#,
    ]);
    let cfg = tmp_cfg();

    let out = agent::run(
        &llm,
        &backend,
        &TerminalApprover,
        &cfg,
        "is DNS-over-TLS on?",
    )
    .await
    .unwrap();
    assert_eq!(
        out.final_answer.as_deref(),
        Some("DNS-over-TLS is on (DoT active, Quad9).")
    );
    assert!(arka_agent::audit::verify(&cfg.audit_path)
        .unwrap()
        .contains("chain intact"));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn pulse_health_result_is_fed_back_as_data() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"pulse_health","args":{}}"#,
        r#"{"final":"System looks healthy."}"#,
    ]);
    let cfg = tmp_cfg();

    let out = agent::run(
        &llm,
        &backend,
        &TerminalApprover,
        &cfg,
        "how is system health?",
    )
    .await
    .unwrap();
    assert!(out.final_answer.is_some());

    // The mock telemetry (temp 51) must have reached the model as a fenced result.
    let seen = llm.seen.lock().unwrap();
    let last = seen.last().unwrap();
    let joined = last
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("51"),
        "telemetry should be fed back to the model"
    );
    assert!(
        joined.contains("TOOL_RESULT"),
        "result should be fenced as data"
    );
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn unknown_tool_is_rejected_and_audited() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        r#"{"tool":"delete_everything","args":{}}"#,
        r#"{"final":"I can only use the listed tools."}"#,
    ]);
    let cfg = tmp_cfg();

    let out = agent::run(&llm, &backend, &TerminalApprover, &cfg, "delete everything")
        .await
        .unwrap();
    assert_eq!(
        out.final_answer.as_deref(),
        Some("I can only use the listed tools.")
    );

    let content = std::fs::read_to_string(&cfg.audit_path).unwrap();
    assert!(content.contains("\"decision\":\"rejected\""));
    assert!(content.contains("delete_everything"));
    assert!(arka_agent::audit::verify(&cfg.audit_path)
        .unwrap()
        .contains("chain intact"));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn invalid_json_is_rejected_then_recovers() {
    let backend = MockBackend::healthy();
    let llm = MockLlm::new(vec![
        "turn off mac randomization", // not JSON → rejected, nothing runs
        r#"{"final":"Understood."}"#,
    ]);
    let cfg = tmp_cfg();

    let out = agent::run(&llm, &backend, &TerminalApprover, &cfg, "hello")
        .await
        .unwrap();
    assert_eq!(out.final_answer.as_deref(), Some("Understood."));
    let _ = std::fs::remove_file(&cfg.audit_path);
}

#[tokio::test]
async fn step_cap_is_enforced() {
    let backend = MockBackend::healthy();
    // Never answers "final" — always calls a read tool. Must stop at 5 steps.
    let scripted: Vec<&str> = (0..10)
        .map(|_| r#"{"tool":"system_status","args":{}}"#)
        .collect();
    let llm = MockLlm::new(scripted);
    let cfg = tmp_cfg();

    let out = agent::run(&llm, &backend, &TerminalApprover, &cfg, "loop forever")
        .await
        .unwrap();
    assert!(out.final_answer.is_none());
    assert_eq!(out.steps_used, 5, "must hard-cap at 5 steps");
    let _ = std::fs::remove_file(&cfg.audit_path);
}
