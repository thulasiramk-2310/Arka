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
        std::fs::create_dir_all("/etc/sysctl.d")?;
        std::fs::write(SYSCTL_CONF, SYSCTL_CONTENT)?;
        crate::command::run("sysctl", &["-p", SYSCTL_CONF]).await?;
        // all/default alone do not update every existing interface.
        for entry in std::fs::read_dir("/proc/sys/net/ipv6/conf")? {
            let path = entry?.path();
            if path.join("disable_ipv6").is_file()
                && std::fs::read_to_string(path.join("disable_ipv6"))?.trim() == "0"
                && path.file_name().and_then(|s| s.to_str()) != Some("lo") {
                std::fs::write(path.join("use_tempaddr"), "2")?;
            }
        }
        Ok(())
    }

    async fn update_state(&self, state: &SharedState) {
        state.write().await.ipv6_privacy = verify_interfaces(std::path::Path::new("/proc/sys/net/ipv6/conf"));
    }
}

fn verify_interfaces(root: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else { return false; };
    let mut seen = false;
    for entry in entries {
        let Ok(entry) = entry else { return false; };
        if entry.file_name() == "lo" { continue; }
        let path = entry.path();
        let Ok(disabled) = std::fs::read_to_string(path.join("disable_ipv6")) else { return false; };
        if disabled.trim() == "1" { continue; }
        if disabled.trim() != "0" { return false; }
        let Ok(value) = std::fs::read_to_string(path.join("use_tempaddr")) else { return false; };
        if value.trim() != "2" { return false; }
        seen = true;
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_existing_interfaces_not_only_the_global_default() {
        let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("arka-ipv6-{id}"));
        for name in ["all", "default", "eth0"] {
            let path = root.join(name);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("disable_ipv6"), "0").unwrap();
            std::fs::write(path.join("use_tempaddr"), "2").unwrap();
        }
        assert!(verify_interfaces(&root));
        std::fs::write(root.join("eth0/use_tempaddr"), "0").unwrap();
        assert!(!verify_interfaces(&root));
        std::fs::write(root.join("eth0/use_tempaddr"), "22").unwrap();
        assert!(!verify_interfaces(&root));
        std::fs::write(root.join("eth0/disable_ipv6"), "1").unwrap();
        assert!(verify_interfaces(&root));
        std::fs::remove_dir_all(root).unwrap();
    }
}
