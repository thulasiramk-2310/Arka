use std::sync::Arc;
use zbus::interface;

use crate::enforcers::AsyncEnforcer;
use crate::state::SharedState;
use crate::reconcile;

pub struct ArkadIface {
    pub authorization_lock: tokio::sync::Mutex<()>,
    pub enforcement_lock: Arc<tokio::sync::Mutex<()>>,
    pub state:     SharedState,
    pub enforcers: Arc<Vec<Box<dyn AsyncEnforcer>>>,
}

#[interface(name = "org.arka.arkad")]
impl ArkadIface {
    // — Properties —

    #[zbus(property(emits_changed_signal = "false"))]
    async fn last_check(&self) -> u64 {
        self.state.read().await.last_check
    }

    #[zbus(property)]
    async fn privacy_score(&self) -> u8 {
        self.state.read().await.privacy_score
    }

    #[zbus(property)]
    async fn mac_randomization(&self) -> bool {
        self.state.read().await.mac_randomization
    }

    #[zbus(property)]
    async fn dns_status(&self) -> String {
        self.state.read().await.dns_status.as_str().to_owned()
    }

    #[zbus(property)]
    async fn hostname_privacy(&self) -> bool {
        self.state.read().await.hostname_privacy
    }

    #[zbus(property)]
    async fn ipv6_privacy(&self) -> bool {
        self.state.read().await.ipv6_privacy
    }

    #[zbus(property)]
    async fn firewall(&self) -> bool {
        self.state.read().await.firewall
    }

    #[zbus(property)]
    async fn sandbox_status(&self) -> String {
        self.state.read().await.sandbox_status.as_str().to_owned()
    }

    #[zbus(property)]
    async fn browser_sandbox(&self) -> String {
        self.state.read().await.browser_sandbox.as_str().to_owned()
    }

    // — Methods —

    async fn enforce_all(
        &self,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        // Bound pending authentication prompts without holding the periodic
        // enforcement mutex while somebody is entering their password.
        let auth_guard = self.authorization_lock.try_lock()
            .map_err(|_| zbus::fdo::Error::Failed("Another authorization request is pending".into()))?;
        super::authorization::check(conn, &header).await?;
        drop(auth_guard);
        let _guard = self.enforcement_lock.try_lock()
            .map_err(|_| zbus::fdo::Error::Failed("Enforcement is already running; try again shortly".into()))?;
        let failed = reconcile::reconcile(&self.enforcers, &self.state, true).await;
        if failed.is_empty() {
            Ok(())
        } else {
            Err(zbus::fdo::Error::Failed(format!("Not confirmed: {}", failed.join(", "))))
        }
    }
}
