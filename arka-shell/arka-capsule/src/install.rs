//! Terminal workflow: user installation, visible failures, literal app ID argv.
pub const SCRIPT: &str = r#"
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo &&
flatpak install --user -y flathub "$1"
result=$?
if [ "$result" -eq 0 ]; then
    printf '%s\n' 'Installation finished.'
else
    printf 'Installation failed (exit %s). Review the error above.\n' "$result"
fi
printf 'Press Enter to close… '
IFS= read -r reply
exit "$result"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    fn run(remote: u8, install: u8, id: &str) -> std::process::Output {
        let mock = format!(r#"flatpak() {{
            printf 'call: <%s>\n' "$@"
            case "$1" in remote-add) return {remote};; install) return {install};; esac
        }}
        {SCRIPT}"#);
        Command::new("sh").args(["-c", &mock, "arka-capsule-install", id])
            .stdin(Stdio::null()).output().unwrap()
    }
    #[test]
    fn configures_user_remote_before_user_install() {
        let o = run(0, 0, "org.example.App");
        assert!(o.status.success());
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(s.find("<remote-add>").unwrap() < s.find("<install>").unwrap());
        assert_eq!(s.matches("<--user>").count(), 2);
        assert!(s.contains("<org.example.App>"));
    }
    #[test]
    fn failed_remote_stops_install_and_preserves_failure() {
        let o = run(23, 0, "org.example.App");
        assert_eq!(o.status.code(), Some(23));
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(!s.contains("<install>"));
        assert!(s.contains("Installation failed"));
        assert!(!s.contains("Installation finished"));
    }
    #[test]
    fn failed_install_is_visible_and_nonzero() {
        let o = run(0, 42, "org.example.App");
        assert_eq!(o.status.code(), Some(42));
        assert!(String::from_utf8(o.stdout).unwrap().contains("Installation failed (exit 42)"));
    }
    #[test]
    fn app_id_is_literal_not_shell_source() {
        let id = "org.example.App; printf INJECTED";
        let o = run(0, 0, id);
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(s.contains(&format!("<{id}>")));
        assert!(!s.lines().any(|line| line == "INJECTED"));
    }
}
