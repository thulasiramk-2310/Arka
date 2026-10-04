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
        let changed = std::fs::read_to_string(NM_CONF).map(|s| s != NM_CONF_CONTENT).unwrap_or(true);
        if changed {
            std::fs::create_dir_all("/etc/NetworkManager/conf.d")?;
            std::fs::write(NM_CONF, NM_CONF_CONTENT)?;
        }
        crate::command::run("nmcli", &["general", "reload"]).await?;
        // Defaults apply on the next connection. Do not disconnect a user's
        // active network to rotate its address during an ordinary health check.
        Ok(())
    }

    async fn update_state(&self, state: &SharedState) {
        let configured = std::fs::read_to_string(NM_CONF).map(|c| c == NM_CONF_CONTENT).unwrap_or(false);
        // No activated Ethernet/Wi-Fi connection exposes no address, so the
        // configuration alone decides. With connections up, every one must be
        // using a private address. A D-Bus failure counts as unconfirmed.
        let active = match active_addresses_randomized().await {
            Ok(v) => v.unwrap_or(true),
            Err(e) => {
                tracing::warn!("mac: could not read NetworkManager devices: {e}");
                false
            }
        };
        state.write().await.mac_randomization = configured && active;
    }
}

/// Some(true) if every activated Ethernet/Wi-Fi device uses a private address,
/// Some(false) if one does not, None if none is activated.
async fn active_addresses_randomized() -> zbus::Result<Option<bool>> {
    let conn = zbus::Connection::system().await?;
    let manager = zbus::Proxy::new(&conn, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager").await?;
    let devices: Vec<zbus::zvariant::OwnedObjectPath> = manager.call("GetDevices", &()).await?;
    let mut seen = false;
    for path in devices {
        let device = zbus::Proxy::new(&conn, "org.freedesktop.NetworkManager", path.as_str(), "org.freedesktop.NetworkManager.Device").await?;
        let kind: u32 = device.get_property("DeviceType").await?;
        let subtype = match kind {
            1 => "org.freedesktop.NetworkManager.Device.Wired",
            2 => "org.freedesktop.NetworkManager.Device.Wireless",
            _ => continue,
        };
        let status: u32 = device.get_property("State").await?;
        if status != 100 { continue; } // NM_DEVICE_STATE_ACTIVATED
        seen = true;
        let hardware = zbus::Proxy::new(&conn, "org.freedesktop.NetworkManager", path.as_str(), subtype).await?;
        let current: String = device.get_property("HwAddress").await?;
        let permanent: String = hardware.get_property("PermHwAddress").await?;
        if !private_address(&current, &permanent) {
            let iface: String = device.get_property("Interface").await.unwrap_or_default();
            tracing::info!(iface, current, permanent, "mac: active connection is not using a private address");
            return Ok(Some(false));
        }
    }
    Ok(seen.then_some(true))
}

fn private_address(current: &str, permanent: &str) -> bool {
    fn parse(s: &str) -> Option<Vec<u8>> {
        let bytes: Option<Vec<_>> = s.split(':').map(|v| {
            if v.len() != 2 { None } else { u8::from_str_radix(v, 16).ok() }
        }).collect();
        bytes.filter(|v| v.len() == 6 && v.iter().any(|b| *b != 0))
    }
    match (parse(current), parse(permanent)) {
        (Some(c), Some(p)) => c != p && c[0] & 3 == 2,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_a_changed_locally_administered_unicast_address() {
        assert!(private_address("02:11:22:33:44:55", "00:11:22:33:44:55"));
        assert!(!private_address("02:11:22:33:44:55", "02:11:22:33:44:55"));
        assert!(!private_address("01:11:22:33:44:55", "00:11:22:33:44:55"));
        assert!(!private_address("02:11:22:33:44:55", ""));
        assert!(!private_address("random", "permanent"));
    }
}
