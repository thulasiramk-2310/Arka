use crate::config::Config;
use super::Enforcer;

const SYSCTL_CONF: &str = "/etc/sysctl.d/99-arkad-ipv6-privacy.conf";
const SYSCTL_CONTENT: &str =
    "net.ipv6.conf.all.use_tempaddr=2\nnet.ipv6.conf.default.use_tempaddr=2\n";

pub struct Ipv6Enforcer;

impl Ipv6Enforcer {
    pub fn new(_cfg: &Config) -> Self { Self }
}

impl Enforcer for Ipv6Enforcer {
    fn name(&self) -> &'static str { "ipv6" }

    fn enforce(&self) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all("/etc/sysctl.d")?;
        std::fs::write(SYSCTL_CONF, SYSCTL_CONTENT)?;
        // Apply immediately via sysctl -p
        let _ = std::process::Command::new("sysctl")
            .args(["-p", SYSCTL_CONF])
            .status();
        Ok(())
    }

    fn verify(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let out = std::process::Command::new("sysctl")
            .arg("net.ipv6.conf.all.use_tempaddr")
            .output()?;
        let val = String::from_utf8_lossy(&out.stdout);
        Ok(val.contains('2'))
    }
}
