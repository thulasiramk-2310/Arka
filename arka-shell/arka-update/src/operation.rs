#[derive(Clone, Copy)]
pub enum Operation {
    Check,
    Upgrade,
    Rollback,
}

impl Operation {
    pub fn script(self) -> String {
        let (command, success) = match self {
            Self::Check => ("sudo bootc upgrade --check", "Check finished. Review the result above."),
            Self::Upgrade => ("sudo bootc upgrade", "Update command finished. Review the result above; reboot to apply a staged update."),
            Self::Rollback => ("sudo bootc rollback", "Previous deployment selected. Reboot to apply. User data is not rolled back."),
        };
        // These are fixed commands, never interpolated user input. Keep both
        // success and failure output visible, and preserve the command's exit code.
        format!("{command}\nresult=$?\nif [ \"$result\" -eq 0 ]; then\n  printf '%s\\n' '{success}'\nelse\n  printf 'Operation failed (exit %s). Review the error above.\\n' \"$result\"\nfi\nprintf 'Press Enter to close… '\nIFS= read -r reply\nexit \"$result\"\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn run(operation: Operation, exit: u8) -> std::process::Output {
        // The shell function replaces sudo completely: no host command runs.
        let script = format!(
            "sudo() {{ printf 'requested: %s\\n' \"$*\"; return {exit}; }}\n{}",
            operation.script()
        );
        Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    #[test]
    fn failures_remain_visible_and_preserve_exit_status() {
        for op in [Operation::Check, Operation::Upgrade, Operation::Rollback] {
            let output = run(op, 42);
            let text = String::from_utf8(output.stdout).unwrap();
            assert_eq!(output.status.code(), Some(42));
            assert!(text.contains("Operation failed (exit 42)"));
            assert!(text.contains("Press Enter"));
            assert!(!text.contains("Previous deployment selected"));
        }
    }

    #[test]
    fn check_does_not_run_an_upgrade() {
        let output = run(Operation::Check, 0);
        assert!(output.status.success());
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("requested: bootc upgrade --check\n"));
    }

    #[test]
    fn rollback_does_not_claim_to_restore_user_data() {
        let output = run(Operation::Rollback, 0);
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("requested: bootc rollback\n"));
        assert!(text.contains("User data is not rolled back"));
    }
}
