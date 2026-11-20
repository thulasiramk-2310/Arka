//! Running the system tools. Every command is run directly (never through a shell), and its
//! full command line goes to the install log.

use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Command, Stdio};

pub const LOG: &str = "/run/arka-install/install.log";

/// Appends a line to the install log. Failing to log never stops an install.
pub fn log(line: &str) {
    eprintln!("{line}");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(LOG) {
        let _ = writeln!(f, "{line}");
    }
}

pub struct Output {
    pub ok: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs a program and captures its output.
pub fn capture(prog: &str, args: &[&str]) -> Result<Output, String> {
    log(&format!("$ {} {}", prog, args.join(" ")));
    let out = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not start {prog}: {e}"))?;
    Ok(Output {
        ok: out.status.success(),
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Like `capture`, with extra environment variables. Their values never reach the log: this is
/// how a PIN is handed to systemd-cryptenroll ($NEWPIN).
pub fn capture_env(prog: &str, args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    log(&format!("$ {} {} {}", env.iter().map(|(k, _)| format!("{k}=…")).collect::<Vec<_>>().join(" "), prog, args.join(" ")));
    let out = Command::new(prog)
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not start {prog}: {e}"))?;
    Ok(Output {
        ok: out.status.success(),
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// An error naming the program and its message, for a failed `Output`.
pub fn failed(prog: &str, o: &Output) -> String {
    format!("{prog} failed (exit {}): {}", o.code.map(|c| c.to_string()).unwrap_or_else(|| "signal".into()), o.stderr.trim())
}

/// Runs a program and returns its standard output, or an error naming the program and its message.
pub fn stdout(prog: &str, args: &[&str]) -> Result<String, String> {
    let o = capture(prog, args)?;
    if o.ok {
        Ok(o.stdout)
    } else {
        Err(format!(
            "{prog} failed (exit {}): {}",
            o.code.map(|c| c.to_string()).unwrap_or_else(|| "signal".into()),
            o.stderr.trim()
        ))
    }
}

/// Runs a program whose output goes straight to the log (long-running steps such as bootc).
pub fn stream(prog: &str, args: &[&str]) -> Result<(), String> {
    log(&format!("$ {} {}", prog, args.join(" ")));
    let logf = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG)
        .map_err(|e| format!("could not open the install log: {e}"))?;
    let status = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(logf.try_clone().map_err(|e| e.to_string())?)
        .stderr(logf)
        .status()
        .map_err(|e| format!("could not start {prog}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{prog} failed (exit {:?}); see {LOG}", status.code()))
    }
}

/// True when this process runs as root.
pub fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(2).map(|u| u == "0"))
        })
        .unwrap_or(false)
}
