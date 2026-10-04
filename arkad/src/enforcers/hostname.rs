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
        crate::command::run("hostnamectl", &["set-hostname", &self.hostname]).await?;
        Ok(())
    }

    async fn update_state(&self, state: &SharedState) {
        let ok = crate::command::run("hostnamectl", &["--static"]).await
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == self.hostname)
            .unwrap_or(false);
        state.write().await.hostname_privacy = ok;
    }
}
