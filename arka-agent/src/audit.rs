//! Edit-detecting audit log (rule 7). One JSONL line per tool call, each line
//! carrying the SHA-256 of the previous line so the chain can be verified.
//!
//! What this catches: partial or accidental edits — a changed field or a spliced
//! line breaks the chain and `verify` reports it. What it does NOT stop: a root
//! user rewriting the whole chain from scratch, recomputing every hash. Resisting
//! that needs the chain head anchored off-box.
//! TODO: anchor the latest hash elsewhere (TPM NV index, a remote log, or print
//! it) so a full rewrite is detectable. Until then this "detects edits", it is
//! not "tamper-proof".
//!
//! A line's `hash` = SHA-256( prev_hash + canonical_payload ), where the
//! payload is every field except `hash`, serialised in a fixed order.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const GENESIS: &str = "genesis";

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub time_ms: u128,
    pub request: String,
    pub tool: String,
    pub args: serde_json::Value,
    /// approved | denied | auto | rejected
    pub decision: String,
    pub result: String,
    pub prev: String,
    pub hash: String,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// The exact pre-image that gets hashed. Fixed field order, `prev` folded in.
fn preimage(
    time_ms: u128,
    request: &str,
    tool: &str,
    args: &serde_json::Value,
    decision: &str,
    result: &str,
    prev: &str,
) -> String {
    serde_json::json!({
        "time_ms": time_ms,
        "request": request,
        "tool": tool,
        "args": args,
        "decision": decision,
        "result": result,
        "prev": prev,
    })
    .to_string()
}

fn sha(pre: &str) -> String {
    let mut h = Sha256::new();
    h.update(pre.as_bytes());
    hex::encode(h.finalize())
}

fn last_hash(path: &Path) -> anyhow::Result<String> {
    if !path.exists() {
        return Ok(GENESIS.into());
    }
    let mut last = GENESIS.to_string();
    for line in BufReader::new(fs::File::open(path)?).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let e: Entry = serde_json::from_str(&line)?;
        last = e.hash;
    }
    Ok(last)
}

/// Append one audited event. Reads and writes are both logged.
pub fn append(
    path: &str,
    request: &str,
    tool: &str,
    args: &serde_json::Value,
    decision: &str,
    result: &str,
) -> anyhow::Result<()> {
    let path = Path::new(path);
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    let prev = last_hash(path)?;
    let time_ms = now_ms();
    let hash = sha(&preimage(
        time_ms, request, tool, args, decision, result, &prev,
    ));
    let entry = Entry {
        time_ms,
        request: request.into(),
        tool: tool.into(),
        args: args.clone(),
        decision: decision.into(),
        result: result.into(),
        prev,
        hash,
    };
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Verify the whole chain. Returns a human report on success; errors on the
/// first broken link (chain break or tampered payload).
pub fn verify(path: &str) -> anyhow::Result<String> {
    let path = Path::new(path);
    if !path.exists() {
        return Ok("audit log: empty (no entries yet) — OK".into());
    }
    let mut prev = GENESIS.to_string();
    let mut n = 0usize;
    for (i, line) in BufReader::new(fs::File::open(path)?).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let e: Entry = serde_json::from_str(&line)
            .map_err(|err| anyhow::anyhow!("line {}: not valid JSON: {err}", i + 1))?;
        if e.prev != prev {
            anyhow::bail!("line {}: prev-hash mismatch — chain broken", i + 1);
        }
        let expect = sha(&preimage(
            e.time_ms,
            &e.request,
            &e.tool,
            &e.args,
            &e.decision,
            &e.result,
            &e.prev,
        ));
        if expect != e.hash {
            anyhow::bail!("line {}: hash mismatch — entry was tampered", i + 1);
        }
        prev = e.hash;
        n += 1;
    }
    Ok(format!(
        "audit log: {n} entr{} — chain intact \u{2713}",
        if n == 1 { "y" } else { "ies" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let mut p = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let i = N.fetch_add(1, Ordering::Relaxed);
        p.push(format!("arka-agent-audit-test-{nanos}-{i}.jsonl"));
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn empty_verifies() {
        let p = tmp();
        assert!(verify(&p).unwrap().contains("OK"));
    }

    #[test]
    fn chain_appends_and_verifies() {
        let p = tmp();
        append(
            &p,
            "req1",
            "system_status",
            &serde_json::json!({}),
            "auto",
            "ok",
        )
        .unwrap();
        append(
            &p,
            "req2",
            "enforce_privacy",
            &serde_json::json!({}),
            "approved",
            "done",
        )
        .unwrap();
        assert!(verify(&p).unwrap().contains("chain intact"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn tamper_is_detected() {
        let p = tmp();
        append(
            &p,
            "req1",
            "system_status",
            &serde_json::json!({}),
            "auto",
            "ok",
        )
        .unwrap();
        // flip the result field in place, leaving the hash stale
        let content = std::fs::read_to_string(&p)
            .unwrap()
            .replace("\"ok\"", "\"HACKED\"");
        std::fs::write(&p, content).unwrap();
        assert!(verify(&p).is_err());
        let _ = std::fs::remove_file(&p);
    }
}
