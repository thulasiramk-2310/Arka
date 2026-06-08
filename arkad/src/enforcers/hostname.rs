use crate::config::Config;
use super::Enforcer;

pub struct HostnameEnforcer {
    name: String,
}

impl HostnameEnforcer {
    pub fn new(cfg: &Config) -> Self {
        Self { name: cfg.hostname.name.clone() }
    }
}

impl Enforcer for HostnameEnforcer {
    fn name(&self) -> &'static str { "hostname" }

    fn enforce(&self) -> Result<(), Box<dyn std::error::Error>> {
        std::process::Command::new("hostnamectl")
            .args(["set-hostname", &self.name])
            .status()?;
        Ok(())
    }

    fn verify(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let out = std::process::Command::new("hostnamectl")
            .arg("--static")
            .output()?;
        Ok(String::from_utf8_lossy(&out.stdout).trim() == self.name)
    }
}
