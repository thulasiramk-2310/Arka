use crate::config::Config;
use super::Enforcer;

const NM_CONF: &str = "/etc/NetworkManager/conf.d/00-arkaos-mac-random.conf";
const NM_CONF_CONTENT: &str = "[device]\nwifi.scan-rand-mac-address=yes\n\n[connection]\nwifi.cloned-mac-address=random\nethernet.cloned-mac-address=random\n";

pub struct MacEnforcer;

impl MacEnforcer {
    pub fn new(_cfg: &Config) -> Self { Self }
}

impl Enforcer for MacEnforcer {
    fn name(&self) -> &'static str { "mac" }

    fn enforce(&self) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all("/etc/NetworkManager/conf.d")?;
        std::fs::write(NM_CONF, NM_CONF_CONTENT)?;
        // Reload NM so the config takes effect on running interfaces
        let _ = std::process::Command::new("nmcli")
            .args(["general", "reload"])
            .status();
        Ok(())
    }

    fn verify(&self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(std::fs::read_to_string(NM_CONF)
            .map(|c| c == NM_CONF_CONTENT)
            .unwrap_or(false))
    }
}
