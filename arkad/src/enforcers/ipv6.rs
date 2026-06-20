use crate::config::Config;
use crate::error::ArkadError;
use crate::state::SharedState;
use super::AsyncEnforcer;

const SYSCTL_CONF: &str = "/etc/sysctl.d/99-arkad-ipv6-privacy.conf";
const SYSCTL_CONTENT: &str =
    "net.ipv6.conf.all.use_tempaddr=2\nnet.ipv6.conf.default.use_tempaddr=2\n";

pub struct Ipv6Enforcer;

impl Ipv6Enforcer {
    pub fn new(_cfg: &Config) -> Self { Self }
}

#[async_trait::async_trait]
impl AsyncEnforcer for Ipv6Enforcer {
    fn name(&self) -> &'static str { "ipv6" }

    async fn enforce(&self) -> Result<(), ArkadError> {
        tokio::task::spawn_blocking(|| {
            std::fs::create_dir_all("/etc/sysctl.d")
                .map_err(|e| ArkadError::Enforce(e.to_string()))?;
            std::fs::write(SYSCTL_CONF, SYSCTL_CONTENT)
                .map_err(|e| ArkadError::Enforce(e.to_string()))?;
            let _ = std::process::Command::new("sysctl")
                .args(["-p", SYSCTL_CONF])
                .status();
            Ok::<(), ArkadError>(())
        }).await?
    }

    async fn update_state(&self, state: &SharedState) {
        let ok = tokio::task::spawn_blocking(|| {
            std::process::Command::new("sysctl")
                .arg("net.ipv6.conf.all.use_tempaddr")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains('2'))
                .unwrap_or(false)
        }).await.unwrap_or(false);
        state.write().await.ipv6_privacy = ok;
    }
}
