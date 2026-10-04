use std::process::Output;
use std::time::Duration;

use crate::error::ArkadError;

/// Bound external helpers and propagate unsuccessful exits. Dropping a timed-out
/// future kills its direct child rather than leaving a stuck helper behind.
pub async fn run(program: &str, args: &[&str]) -> Result<Output, ArkadError> {
    run_with_timeout(program, args, Duration::from_secs(5)).await
}

async fn run_with_timeout(program: &str, args: &[&str], limit: Duration) -> Result<Output, ArkadError> {
    let output = tokio::time::timeout(
        limit,
        tokio::process::Command::new(program)
            .args(args)
            .env("LC_ALL", "C")
            .env("SYSTEMD_COLORS", "0")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| ArkadError::Enforce(format!("{program} timed out")))??;
    if !output.status.success() {
        return Err(ArkadError::Enforce(format!("{program} exited with {}", output.status)));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_failed_commands_and_keeps_successful_output() {
        assert!(run("sh", &["-c", "exit 7"]).await.is_err());
        assert_eq!(run("printf", &["ok"]).await.unwrap().stdout, b"ok");
    }

    #[tokio::test]
    async fn times_out_a_stuck_child() {
        assert!(run_with_timeout("sleep", &["5"], Duration::from_millis(20)).await.is_err());
    }
}
