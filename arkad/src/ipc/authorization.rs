use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use zbus::{message::Header, zvariant::Value, Connection};

const ACTION: &str = "org.arka.arkad.enforce";
const AUTHORITY: &str = "org.freedesktop.PolicyKit1";
const PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";
static REQUEST_ID: AtomicU64 = AtomicU64::new(0);

#[derive(serde::Deserialize, serde::Serialize, zbus::zvariant::Type)]
struct AuthorizationResult {
    authorized: bool,
    challenge: bool,
    details: HashMap<String, String>,
}

pub async fn check(conn: &Connection, header: &Header<'_>) -> zbus::fdo::Result<()> {
    let sender = header
        .sender()
        .ok_or_else(|| denied("Missing authenticated bus sender"))?;
    let interactive = header
        .primary()
        .flags()
        .contains(zbus::message::Flags::AllowInteractiveAuth);
    check_sender(conn, sender.as_str(), interactive, Duration::from_secs(60)).await
}

fn denied(message: &str) -> zbus::fdo::Error {
    zbus::fdo::Error::AccessDenied(message.into())
}

async fn check_sender(
    conn: &Connection,
    sender: &str,
    interactive: bool,
    timeout: Duration,
) -> zbus::fdo::Result<()> {
    // This identity comes from the message bus header, never method arguments
    // or a caller-supplied PID (which could be reused before authorization).
    let subject = (
        "system-bus-name",
        HashMap::from([("name", Value::from(sender))]),
    );
    let cancellation = format!(
        "arka-enforce-{}",
        REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    );
    let args = (
        subject,
        ACTION,
        HashMap::<&str, &str>::new(),
        u32::from(interactive),
        &cancellation,
    );
    let call = conn.call_method(
        Some(AUTHORITY),
        PATH,
        Some(INTERFACE),
        "CheckAuthorization",
        &args,
    );
    let response = match tokio::time::timeout(timeout, call).await {
        Ok(Ok(reply)) => reply,
        Ok(Err(_)) => return Err(denied("Authorization service unavailable; no changes made")),
        Err(_) => {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                conn.call_method(
                    Some(AUTHORITY),
                    PATH,
                    Some(INTERFACE),
                    "CancelCheckAuthorization",
                    &(&cancellation,),
                ),
            )
            .await;
            return Err(denied(
                "Administrator authentication timed out; no changes made",
            ));
        }
    };
    let (result,): (AuthorizationResult,) = response
        .body()
        .deserialize()
        .map_err(|_| denied("Invalid authorization reply; no changes made"))?;
    if result.authorized {
        Ok(())
    } else {
        Err(denied(
            "Administrator authentication is required to reapply privacy settings",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };
    use zbus::zvariant::OwnedValue;

    struct TestBus(Child);
    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[derive(Default)]
    struct Replies {
        allow: bool,
        delay: Duration,
        cancelled: usize,
        calls: Vec<(String, String, u32)>,
    }
    struct Authority(Arc<Mutex<Replies>>);

    #[zbus::interface(name = "org.freedesktop.PolicyKit1.Authority")]
    impl Authority {
        async fn check_authorization(
            &self,
            subject: (String, HashMap<String, OwnedValue>),
            action: String,
            _details: HashMap<String, String>,
            flags: u32,
            _cancellation: String,
        ) -> AuthorizationResult {
            assert_eq!(subject.0, "system-bus-name");
            let sender = <&str>::try_from(subject.1.get("name").unwrap())
                .unwrap()
                .to_owned();
            let (allow, delay) = {
                let mut state = self.0.lock().unwrap();
                state.calls.push((sender, action, flags));
                (state.allow, state.delay)
            };
            tokio::time::sleep(delay).await;
            AuthorizationResult {
                authorized: allow,
                challenge: !allow,
                details: HashMap::new(),
            }
        }

        fn cancel_check_authorization(&self, _id: String) {
            self.0.lock().unwrap().cancelled += 1;
        }
    }

    struct FakeEnforcer(Arc<AtomicU64>);
    #[async_trait::async_trait]
    impl crate::enforcers::AsyncEnforcer for FakeEnforcer {
        fn name(&self) -> &'static str {
            "hostname"
        }
        async fn enforce(&self) -> Result<(), crate::error::ArkadError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn update_state(&self, state: &crate::state::SharedState) {
            state.write().await.hostname_privacy = true;
        }
    }

    #[tokio::test]
    async fn checks_actual_caller_and_denies_challenge_or_missing_authority() {
        // An isolated bus and fake authority. No host bus or real enforcer runs.
        let mut bus = TestBus(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .env_remove("XDG_RUNTIME_DIR")
                .stdout(Stdio::piped())
                .spawn()
                .expect("private test bus"),
        );
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let address = address.trim();
        assert!(
            !address.is_empty(),
            "private D-Bus socket could not be created"
        );
        let replies = Arc::new(Mutex::new(Replies::default()));
        let authority = zbus::connection::Builder::address(address)
            .unwrap()
            .name(AUTHORITY)
            .unwrap()
            .serve_at(PATH, Authority(replies.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let state = Arc::new(tokio::sync::RwLock::new(crate::state::ArkadState::default()));
        state.write().await.hostname_privacy = true;
        let mutations = Arc::new(AtomicU64::new(0));
        let server = zbus::connection::Builder::address(address)
            .unwrap()
            .name("org.arka.arkad")
            .unwrap()
            .serve_at(
                "/org/arka/arkad",
                super::super::ArkadIface {
                    state,
                    enforcers: Arc::new(vec![Box::new(FakeEnforcer(mutations.clone()))]),
                    enforcement_lock: Arc::new(tokio::sync::Mutex::new(())),
                    authorization_lock: tokio::sync::Mutex::new(()),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = zbus::connection::Builder::address(address)
            .unwrap()
            .build()
            .await
            .unwrap();
        let proxy = zbus::proxy::Builder::<zbus::Proxy<'_>>::new(&client)
            .destination("org.arka.arkad").unwrap()
            .path("/org/arka/arkad").unwrap()
            .interface("org.arka.arkad").unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build().await.unwrap();
        // Read-only properties remain accessible without prompting.
        assert_eq!(proxy.get_property::<u8>("PrivacyScore").await.unwrap(), 0);
        assert_eq!(proxy.get_property::<u64>("LastCheck").await.unwrap(), 0);
        assert!(proxy.set_property("LastCheck", &123u64).await.is_err());
        assert!(replies.lock().unwrap().calls.is_empty());
        assert!(proxy.call::<_, _, ()>("EnforceAll", &()).await.is_err());
        assert_eq!(mutations.load(Ordering::SeqCst), 0);
        {
            let mut reply = replies.lock().unwrap();
            assert_eq!(
                reply.calls[0],
                (client.unique_name().unwrap().to_string(), ACTION.into(), 0)
            );
            reply.allow = true;
        }
        let result: Option<()> = proxy
            .call_with_flags(
                "EnforceAll",
                zbus::proxy::MethodFlags::AllowInteractiveAuth.into(),
                &(),
            )
            .await
            .unwrap();
        assert!(result.is_some());
        assert!(proxy.get_property::<u64>("LastCheck").await.unwrap() > 0);
        assert_eq!(mutations.load(Ordering::SeqCst), 1);
        assert_eq!(replies.lock().unwrap().calls[1].2, 1);
        replies.lock().unwrap().delay = Duration::from_millis(100);
        assert!(check_sender(
            &server,
            client.unique_name().unwrap().as_str(),
            true,
            Duration::from_millis(5)
        )
        .await
        .is_err());
        assert_eq!(replies.lock().unwrap().cancelled, 1);
        authority.close().await.unwrap();
        assert!(proxy.call::<_, _, ()>("EnforceAll", &()).await.is_err());
        assert_eq!(mutations.load(Ordering::SeqCst), 1);
        drop(server);
    }
}
