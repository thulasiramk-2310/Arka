use arka_shell_common::{BrowserSandbox, SandboxStatus};
use crate::error::ArkadError;
use crate::state::SharedState;
use super::AsyncEnforcer;

pub struct SandboxEnforcer;

#[async_trait::async_trait]
impl AsyncEnforcer for SandboxEnforcer {
    fn name(&self) -> &'static str { "sandbox" }

    async fn enforce(&self) -> Result<(), ArkadError> {
        // Sandbox is baked in at build time via the Containerfile.
        // Nothing to enforce at runtime.
        Ok(())
    }

    async fn update_state(&self, state: &SharedState) {
        let has_bwrap = tokio::process::Command::new("bwrap")
            .arg("--version")
            .output().await
            .map(|o| o.status.success())
            .unwrap_or(false);

        let has_wrapper   = std::path::Path::new("/usr/bin/firefox-sandbox").exists();
        let has_symlink   = std::fs::symlink_metadata("/usr/bin/firefox")
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let has_unwrapped = std::path::Path::new("/usr/bin/firefox-unwrapped").exists();

        let browser = if has_wrapper && has_symlink && has_unwrapped {
            BrowserSandbox::Persistent
        } else {
            BrowserSandbox::None
        };
        let status = if has_bwrap { SandboxStatus::Active } else { SandboxStatus::Inactive };

        let mut s = state.write().await;
        s.sandbox_status = status;
        s.browser_sandbox = browser;
    }
}
