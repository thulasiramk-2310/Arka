use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_FILE: &str = "/var/log/arkaos/privacy.jsonl";
/// Past this size the log is moved aside (one older copy is kept) and started again, so a
/// setting that keeps drifting cannot fill the disk. About ten thousand events.
const MAX_BYTES: u64 = 1024 * 1024;

pub fn log_event(cat: &str, ev: &str, msg: &str) {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    append(Path::new(LOG_FILE), MAX_BYTES, &line(ts, cat, ev, msg));
}

fn line(ts: u64, cat: &str, ev: &str, msg: &str) -> String {
    format!("{{\"ts\":{ts},\"cat\":\"{}\",\"ev\":\"{}\",\"msg\":\"{}\"}}\n", escape(cat), escape(ev), escape(msg))
}

/// Text as it may stand inside a JSON string.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn append(file: &Path, max_bytes: u64, line: &str) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(file).is_ok_and(|meta| meta.len() >= max_bytes) {
        let mut older = file.as_os_str().to_owned();
        older.push(".1");
        let _ = std::fs::rename(file, older);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = f.write_all(line.as_bytes());
        if let Ok(meta) = f.metadata() {
            let mut perms = meta.permissions();
            perms.set_mode(0o644);
            let _ = f.set_permissions(perms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_valid_json_whatever_the_text() {
        assert_eq!(line(7, "dns", "drift", "plain"), "{\"ts\":7,\"cat\":\"dns\",\"ev\":\"drift\",\"msg\":\"plain\"}\n");
        assert_eq!(escape("a\"b\\c\nd\u{1}"), "a\\\"b\\\\c\\nd\\u0001");
    }

    #[test]
    fn a_full_log_is_moved_aside_once_and_started_again() {
        let dir = std::env::temp_dir().join(format!("arkad-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("privacy.jsonl");
        for n in 0..5 {
            append(&file, 20, &format!("line number {n}\n"));
        }
        // each line is 14 bytes: two fit before the limit is reached
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "line number 4\n");
        assert_eq!(std::fs::read_to_string(dir.join("privacy.jsonl.1")).unwrap(), "line number 2\nline number 3\n");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o644);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
