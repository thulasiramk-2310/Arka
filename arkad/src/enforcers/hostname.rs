use crate::config::Config;
use crate::error::ArkadError;
use crate::state::SharedState;
use super::AsyncEnforcer;

pub struct HostnameEnforcer {
    hostname: String,
}

impl HostnameEnforcer {
    pub fn new(cfg: &Config) -> Self {
        Self { hostname: cfg.hostname.name.clone() }
    }
}

#[async_trait::async_trait]
impl AsyncEnforcer for HostnameEnforcer {
    fn name(&self) -> &'static str { "hostname" }

    async fn enforce(&self) -> Result<(), ArkadError> {
        let name = self.hostname.clone();
        tokio::task::spawn_blocking(move || {
            std::process::Command::new("hostnamectl")
                .args(["set-hostname", &name])
                .status()
                .map_err(|e| ArkadError::Enforce(e.to_string()))?;
            Ok::<(), ArkadError>(())
        }).await?
    }

    async fn update_state(&self, state: &SharedState) {
        let name = self.hostname.clone();
        let ok = tokio::task::spawn_blocking(move || {
            std::process::Command::new("hostnamectl")
                .arg("--static")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == name)
                .unwrap_or(false)
        }).await.unwrap_or(false);
        state.write().await.hostname_privacy = ok;
    }
}
