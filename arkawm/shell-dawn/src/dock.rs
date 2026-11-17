//! The theme's dock (`[dock] command`): Crystal Dock in the Classic theme. Started with the
//! desktop when the theme has one, left running when the desktop starts again with the same
//! theme, stopped when a theme without a dock is picked. Its process id is kept in
//! `$XDG_RUNTIME_DIR/arka-desk-dock`, with the command that started it.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn record() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("arka-desk-dock"))
}

/// "pid command" from the record file.
fn parse(text: &str) -> Option<(u32, &str)> {
    let (pid, command) = text.trim().split_once(' ')?;
    Some((pid.parse().ok()?, command))
}

/// Whether that process still runs. The record goes with the runtime folder at logout, so a
/// number in it is never one from an earlier session.
fn alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
        && std::fs::read_to_string(format!("/proc/{pid}/status")).is_ok_and(|s| !s.contains("State:\tZ"))
}

pub fn follow(command: &str) {
    let Some(file) = record() else { return };
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let running = parse(&text).filter(|(pid, _)| alive(*pid));
    if let Some((pid, was)) = running {
        if was == command {
            return;
        }
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    let _ = std::fs::remove_file(&file);
    if command.is_empty() {
        return;
    }
    match Command::new("sh").args(["-c", &format!("exec {command}")]).stdin(Stdio::null()).spawn() {
        Ok(child) => {
            let _ = std::fs::write(&file, format!("{} {command}\n", child.id()));
        }
        Err(err) => eprintln!("arka-desk: could not start the dock ({command}): {err}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_record_is_read() {
        assert_eq!(super::parse("4242 /usr/libexec/arkawm/arka-classic-dock\n"), Some((4242, "/usr/libexec/arkawm/arka-classic-dock")));
        assert_eq!(super::parse(""), None);
        assert_eq!(super::parse("x y"), None);
    }
}
