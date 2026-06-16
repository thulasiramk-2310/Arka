use std::sync::Arc;
use zbus::interface;

use crate::enforcers::Enforcer;

pub struct ArkadIface {
    enforcers: Arc<Vec<Box<dyn Enforcer>>>,
}

impl ArkadIface {
    fn status(&self, name: &str) -> bool {
        self.enforcers
            .iter()
            .find(|e| e.name() == name)
            .and_then(|e| e.verify().ok())
            .unwrap_or(false)
    }
}

#[interface(name = "org.arka.arkad")]
impl ArkadIface {
    #[zbus(property)]
    fn mac_randomization(&self) -> bool {
        self.status("mac")
    }

    #[zbus(property)]
    fn dns_encrypted(&self) -> bool {
        self.status("dns")
    }

    #[zbus(property)]
    fn hostname_privacy(&self) -> bool {
        self.status("hostname")
    }

    #[zbus(property)]
    fn ipv6_privacy(&self) -> bool {
        self.status("ipv6")
    }

    #[zbus(property)]
    fn privacy_score(&self) -> u32 {
        const TRACKED: &[&str] = &["mac", "dns", "hostname", "ipv6"];
        let hits = TRACKED.iter().filter(|n| self.status(n)).count();
        (hits * 100 / TRACKED.len()) as u32
    }

    fn enforce_all(&self) {
        for e in self.enforcers.iter() {
            let _ = e.enforce();
        }
    }
}

/// Starts the system-bus service and returns the connection. The connection
/// must be kept alive by the caller (its internal executor thread serves
/// requests for as long as it isn't dropped).
pub fn serve(enforcers: Arc<Vec<Box<dyn Enforcer>>>) -> zbus::Result<zbus::blocking::Connection> {
    zbus::blocking::connection::Builder::system()?
        .name("org.arka.arkad")?
        .serve_at("/org/arka/arkad", ArkadIface { enforcers })?
        .build()
}
