use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{enforcers::AsyncEnforcer, score, state::SharedState};

async fn observe(e: &dyn AsyncEnforcer, state: &SharedState) {
    if tokio::time::timeout(Duration::from_secs(15), e.update_state(state)).await.is_err() {
        state.write().await.invalidate(e.name());
    }
}

/// Called under one shared mutex by the periodic loop and the D-Bus method.
/// Every failed control is retried, regardless of the aggregate score.
pub async fn reconcile(enforcers: &[Box<dyn AsyncEnforcer>], state: &SharedState, force: bool) -> Vec<String> {
    let mut failed = Vec::new();
    for e in enforcers {
        let was_healthy = state.read().await.control_healthy(e.name());
        observe(e.as_ref(), state).await;
        let healthy = state.read().await.control_healthy(e.name());
        if was_healthy && !healthy {
            crate::log::log_event(e.name(), "drift", "Protection could not be confirmed; retrying");
        }
        if force || !healthy {
            let applied = tokio::time::timeout(Duration::from_secs(15), e.enforce()).await;
            let apply_ok = matches!(applied, Ok(Ok(())));
            if !apply_ok {
                tracing::warn!(enforcer = e.name(), "enforcement failed or timed out");
            }
            observe(e.as_ref(), state).await;
            let verified = state.read().await.control_healthy(e.name());
            if !apply_ok || !verified {
                failed.push(e.name().to_owned());
            } else if !healthy {
                crate::log::log_event(e.name(), "recovered", "Protection verified after enforcement");
            }
        }
        // A slow control must not starve the service watchdog between controls.
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
    }
    let mut s = state.write().await;
    s.privacy_score = score::compute(&s);
    s.last_check = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use crate::{error::ArkadError, state::ArkadState};

    struct FailingControl(Arc<AtomicUsize>);
    #[async_trait::async_trait]
    impl AsyncEnforcer for FailingControl {
        fn name(&self) -> &'static str { "hostname" }
        async fn enforce(&self) -> Result<(), ArkadError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(ArkadError::Enforce("test failure".into()))
        }
        async fn update_state(&self, s: &SharedState) { s.write().await.hostname_privacy = false; }
    }

    #[tokio::test]
    async fn unchanged_failure_is_retried_and_returned_to_the_caller() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let es: Vec<Box<dyn AsyncEnforcer>> = vec![Box::new(FailingControl(attempts.clone()))];
        let s = Arc::new(tokio::sync::RwLock::new(ArkadState::default()));
        for _ in 0..3 {
            assert_eq!(reconcile(&es, &s, false).await, ["hostname"]);
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert!(!s.read().await.hostname_privacy);
        assert!(s.read().await.last_check > 0);
    }
}

#[cfg(test)]
mod freshness_tests {
    use super::*;
    use std::sync::Arc;
    use crate::{error::ArkadError, state::ArkadState};

    struct BlockedControl(Arc<tokio::sync::Notify>);
    #[async_trait::async_trait]
    impl AsyncEnforcer for BlockedControl {
        fn name(&self) -> &'static str { "hostname" }
        async fn enforce(&self) -> Result<(), ArkadError> { Ok(()) }
        async fn update_state(&self, state: &SharedState) {
            self.0.notify_one();
            std::future::pending::<()>().await;
            state.write().await.hostname_privacy = true;
        }
    }

    #[tokio::test]
    async fn unfinished_or_cancelled_reconcile_does_not_refresh_timestamp() {
        let state = Arc::new(tokio::sync::RwLock::new(ArkadState::default()));
        assert_eq!(state.read().await.last_check, 0);
        reconcile(&[], &state, false).await;
        let completed = state.read().await.last_check;
        assert!(completed > 0);
        let started = Arc::new(tokio::sync::Notify::new());
        let enforcers: Vec<Box<dyn AsyncEnforcer>> = vec![Box::new(BlockedControl(started.clone()))];
        let copy = state.clone();
        let task = tokio::spawn(async move { reconcile(&enforcers, &copy, false).await });
        started.notified().await;
        assert_eq!(state.read().await.last_check, completed);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(state.read().await.last_check, completed);
    }
}
