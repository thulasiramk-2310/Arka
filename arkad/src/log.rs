use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_FILE: &str = "/var/log/arkaos/privacy.jsonl";

pub fn log_event(cat: &str, ev: &str, msg: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let line = format!("{{\"ts\":{ts},\"cat\":\"{cat}\",\"ev\":\"{ev}\",\"msg\":\"{msg}\"}}\n");

    let _ = std::fs::create_dir_all("/var/log/arkaos");

    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)
    {
        let _ = f.write_all(line.as_bytes());
        if let Ok(meta) = std::fs::metadata(LOG_FILE) {
            let mut perms = meta.permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(LOG_FILE, perms);
        }
    }
}
