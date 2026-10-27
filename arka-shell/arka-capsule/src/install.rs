//! Terminal workflow: user installation, visible failures, literal app ID argv.
pub const SCRIPT: &str = r#"
arka install "$1"
result=$?
case "$result" in
    0) printf '%s\n' 'Installation finished.' ;;
    3) printf '%s\n' 'Nothing was installed.' ;;
    *) printf 'Installation failed (exit %s). Review the error above.\n' "$result" ;;
esac
printf 'Press Enter to close… '
IFS= read -r reply
exit "$result"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    // `arka` is the ArkaOS install command (apps/arka-install): it shows what the app can
    // reach, asks, and installs for the user's account only. Here it is a stand-in.
    fn run(arka_exit: u8, id: &str) -> std::process::Output {
        let mock = format!(r#"arka() {{
            printf 'call: <%s>\n' "$@"
            return {arka_exit}
        }}
        {SCRIPT}"#);
        Command::new("sh").args(["-c", &mock, "arka-capsule-install", id])
            .stdin(Stdio::null()).output().unwrap()
    }
    #[test]
    fn installs_through_the_arka_command() {
        let o = run(0, "org.example.App");
        assert!(o.status.success());
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(s.contains("<install>") && s.contains("<org.example.App>"));
        assert!(s.contains("Installation finished"));
    }
    #[test]
    fn saying_no_is_reported_as_nothing_installed() {
        let o = run(3, "org.example.App");
        assert_eq!(o.status.code(), Some(3));
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(s.contains("Nothing was installed"));
        assert!(!s.contains("Installation finished") && !s.contains("Installation failed"));
    }
    #[test]
    fn a_failed_install_is_visible_and_nonzero() {
        let o = run(42, "org.example.App");
        assert_eq!(o.status.code(), Some(42));
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(s.contains("Installation failed (exit 42)"));
        assert!(!s.contains("Installation finished"));
    }
    #[test]
    fn app_id_is_literal_not_shell_source() {
        let id = "org.example.App; printf INJECTED";
        let o = run(0, id);
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(s.contains(&format!("<{id}>")));
        assert!(!s.lines().any(|line| line == "INJECTED"));
    }
}
