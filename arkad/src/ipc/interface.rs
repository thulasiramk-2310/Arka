use std::sync::Arc;
use zbus::interface;
use tracing::error;

use crate::enforcers::AsyncEnforcer;
use crate::state::SharedState;
use crate::score;

pub struct ArkadIface {
    pub state:     SharedState,
    pub enforcers: Arc<Vec<Box<dyn AsyncEnforcer>>>,
}

#[interface(name = "org.arka.arkad")]
impl ArkadIface {
    // — Properties —

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
    async fn sandbox_status(&self) -> String {
        self.state.read().await.sandbox_status.as_str().to_owned()
    }

    #[zbus(property)]
    async fn browser_sandbox(&self) -> String {
        self.state.read().await.browser_sandbox.as_str().to_owned()
    }

    // — Methods —

    async fn enforce_all(&self) -> zbus::fdo::Result<()> {
        for e in self.enforcers.iter() {
            if let Err(err) = e.enforce().await {
                error!(enforcer = e.name(), %err, "enforce_all: enforce failed");
            }
        }
        for e in self.enforcers.iter() {
            e.update_state(&self.state).await;
        }
        let mut s = self.state.write().await;
        s.privacy_score = score::compute(&s);
        Ok(())
    }
}
