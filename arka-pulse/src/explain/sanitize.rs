//! DataSanitizer — redact secrets before incident context can reach a model.
//!
//! Even though the current [`FallbackExplainer`](super::FallbackExplainer) is
//! local and deterministic, sanitisation is applied unconditionally: it is a
//! discipline, not a model-specific step. When a local LLM backend is added,
//! *nothing* reaches it that has not been through here first.
//!
//! This is best-effort defence in depth, std-only (no regex crate). It favours
//! over-redaction: a false redaction costs a little readability; a leaked token
//! costs far more.

/// Words that mark the *value* after them (or after `=`/`:`) as a secret.
const SECRET_KEYS: &[&str] = &[
    "password", "passwd", "secret", "token", "apikey", "api_key", "access_key",
    "private_key", "authorization", "bearer",
];

/// Known high-signal secret prefixes; the whole run is dropped.
const SECRET_PREFIXES: &[&str] = &[
    "ghp_", "gho_", "ghs_", "github_pat_", "xoxb-", "xoxp-", "sk-", "AKIA", "ASIA",
];

/// Redact a single line of context. Applied per whitespace-separated token so
/// surrounding words survive.
pub fn sanitize(input: &str) -> String {
    input
        .lines()
        .map(sanitize_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_line(line: &str) -> String {
    let mut prev_key_is_secret = false;
    let out: Vec<String> = line
        .split_whitespace()
        .map(|tok| {
            let redacted = redact_token(tok, prev_key_is_secret);
            // If this token is a bare secret key name (e.g. "password:"), the
            // *next* token is its value.
            prev_key_is_secret = is_secret_key_word(tok);
            redacted
        })
        .collect();
    out.join(" ")
}

fn is_secret_key_word(tok: &str) -> bool {
    let t = tok.trim_end_matches([':', '=']).to_ascii_lowercase();
    SECRET_KEYS.contains(&t.as_str())
}

fn redact_token(tok: &str, prev_key_is_secret: bool) -> String {
    if prev_key_is_secret {
        return "<redacted>".to_string();
    }

    // key=value / key:value where the key names a secret.
    for sep in ['=', ':'] {
        if let Some(idx) = tok.find(sep) {
            let (k, v) = tok.split_at(idx);
            if !v[1..].is_empty() && SECRET_KEYS.contains(&k.to_ascii_lowercase().as_str()) {
                return format!("{k}{sep}<redacted>");
            }
        }
    }

    // Known secret prefixes.
    for p in SECRET_PREFIXES {
        if tok.starts_with(p) {
            return "<redacted>".to_string();
        }
    }

    // Home paths: /home/<user>/... -> /home/<user>/...
    if let Some(red) = redact_home_path(tok) {
        return red;
    }

    // Long high-entropy runs (>= 32 token-ish chars) look like keys/hashes.
    if looks_like_secret_blob(tok) {
        return "<redacted>".to_string();
    }

    tok.to_string()
}

fn redact_home_path(tok: &str) -> Option<String> {
    let idx = tok.find("/home/")?;
    let after = &tok[idx + "/home/".len()..];
    let mut end = 0;
    for (i, c) in after.char_indices() {
        if c == '/' {
            end = i;
            break;
        }
        end = i + c.len_utf8();
    }
    if end == 0 {
        return None;
    }
    Some(format!(
        "{}/home/<user>{}",
        &tok[..idx],
        &after[end..]
    ))
}

fn looks_like_secret_blob(tok: &str) -> bool {
    if tok.len() < 32 {
        return false;
    }
    let ok = tok
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'));
    // Require a mix of letters and digits so ordinary long words aren't hit.
    let has_alpha = tok.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = tok.chars().any(|c| c.is_ascii_digit());
    ok && has_alpha && has_digit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_key_value_secret() {
        assert_eq!(sanitize("password=hunter2 foo=bar"), "password=<redacted> foo=bar");
        assert_eq!(sanitize("token: abcdef"), "token: <redacted>");
    }

    #[test]
    fn redacts_known_prefixes() {
        let out = sanitize("using ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 now");
        assert!(out.contains("<redacted>"));
        assert!(!out.contains("ghp_ABCDEF"));
    }

    #[test]
    fn redacts_home_username() {
        assert_eq!(
            sanitize("/home/thulasi/.config/app"),
            "/home/<user>/.config/app"
        );
    }

    #[test]
    fn redacts_long_blob_but_keeps_prose() {
        let out = sanitize("cpu load is high on the server today");
        assert_eq!(out, "cpu load is high on the server today");
        let out2 = sanitize("key aG9sZDEyMzQ1Njc4OTBhYmNkZWZnaGlqaw99");
        assert!(out2.contains("<redacted>"));
    }
}
