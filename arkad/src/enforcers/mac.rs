use crate::config::Config;
use crate::error::ArkadError;
use crate::state::SharedState;
use super::AsyncEnforcer;

const NM_CONF: &str = "/etc/NetworkManager/conf.d/00-arkaos-mac-random.conf";
const NM_CONF_CONTENT: &str = "[device]\nwifi.scan-rand-mac-address=yes\n\n[connection]\nwifi.cloned-mac-address=random\nethernet.cloned-mac-address=random\n";

pub struct MacEnforcer;

impl MacEnforcer {
    pub fn new(_cfg: &Config) -> Self { Self }
}

#[async_trait::async_trait]
impl AsyncEnforcer for MacEnforcer {
    fn name(&self) -> &'static str { "mac" }

    async fn enforce(&self) -> Result<(), ArkadError> {
        tokio::task::spawn_blocking(|| {
            std::fs::create_dir_all("/etc/NetworkManager/conf.d")
                .map_err(|e| ArkadError::Enforce(e.to_string()))?;
            std::fs::write(NM_CONF, NM_CONF_CONTENT)
                .map_err(|e| ArkadError::Enforce(e.to_string()))?;
            let _ = std::process::Command::new("nmcli")
                .args(["general", "reload"])
                .status();
            crate::log::log_event("mac", "active", "MAC address randomization enabled — device can't be tracked");
            Ok::<(), ArkadError>(())
        }).await?
    }

    async fn update_state(&self, state: &SharedState) {
        let ok = tokio::task::spawn_blocking(|| {
            std::fs::read_to_string(NM_CONF)
                .map(|c| c == NM_CONF_CONTENT)
                .unwrap_or(false)
        }).await.unwrap_or(false);
        state.write().await.mac_randomization = ok;
    }
}
