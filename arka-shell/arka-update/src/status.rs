//! Parse `bootc status --json --format-version=1` without invoking bootc.
//! Schema: https://github.com/bootc-dev/bootc/blob/main/crates/lib/src/spec.rs
//! Versions come from image metadata, never guessed from mutable image tags.
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    pub version: Option<String>,
    pub image: Option<String>,
    pub download_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    pub installed: Deployment,
    pub staged: Option<Deployment>,
    pub restart_needed: bool,
    /// A previous deployment exists and the medium permits changing boot order.
    /// This does not certify that the previous deployment will boot successfully.
    pub rollback_available: bool,
    pub rollback_queued: bool,
    pub read_only: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Host {
    api_version: String,
    kind: String,
    #[serde(default)]
    spec: Spec,
    status: Status,
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec { boot_order: Option<String> }
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    booted: Option<Entry>,
    staged: Option<Entry>,
    rollback: Option<Entry>,
    rollback_queued: Option<bool>,
    #[serde(default)]
    read_only: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    image: Option<Image>,
    // Required in format v1: reject malformed entries such as {}.
    #[serde(rename = "incompatible")]
    _incompatible: bool,
    #[serde(rename = "pinned")]
    _pinned: bool,
    #[serde(default)]
    download_only: bool,
}
#[derive(Debug, Deserialize)]
struct Image { image: ImageReference, version: Option<String> }
#[derive(Debug, Deserialize)]
struct ImageReference { image: String }

impl From<Entry> for Deployment {
    fn from(entry: Entry) -> Self {
        let (version, image) = match entry.image {
            Some(image) => (image.version.filter(|v| !v.trim().is_empty()), Some(image.image.image)),
            None => (None, None),
        };
        Self { version, image, download_only: entry.download_only }
    }
}

/// Invalid, unsupported or non-bootc output is an error, never "up to date".
/// Cached update metadata alone does not represent a staged deployment.
/// Where arka-bootc-status.service publishes `bootc status` for unprivileged readers.
pub const PUBLISHED: &str = "/run/arkaos/bootc-status.json";

/// Read the status published by arka-bootc-status.service (no privileges needed).
pub fn read_published() -> Result<UpdateStatus, String> {
    read_published_at(std::path::Path::new(PUBLISHED), std::time::SystemTime::now())
}

// The publisher runs every ten minutes. Allow scheduling jitter, not an
// indefinitely trusted snapshot after a failed publication.
const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(720);
fn read_published_at(path: &std::path::Path, now: std::time::SystemTime) -> Result<UpdateStatus, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("update status unavailable ({e})"))?;
    let modified = file.metadata().and_then(|m| m.modified()).map_err(|e| e.to_string())?;
    let age = now.duration_since(modified).map_err(|_| "update status timestamp is in the future")?;
    if age >= MAX_AGE { return Err("update status is stale; the publisher has not refreshed it".into()); }
    let mut json = String::new();
    file.read_to_string(&mut json).map_err(|e| e.to_string())?;
    parse_status(&json)
}

pub fn parse_status(json: &str) -> Result<UpdateStatus, String> {
    let host: Host = serde_json::from_str(json).map_err(|e| format!("Invalid bootc status: {e}"))?;
    if host.api_version != "org.containers.bootc/v1" || host.kind != "BootcHost" {
        return Err("Unsupported bootc status format".into());
    }
    let status = host.status;
    let installed = status.booted.ok_or("No booted bootc deployment")?.into();
    let rollback_queued = status.rollback_queued.unwrap_or(host.spec.boot_order.as_deref() == Some("rollback"));
    if rollback_queued && status.rollback.is_none() {
        return Err("Rollback queued without a rollback deployment".into());
    }
    let staged: Option<Deployment> = status.staged.map(Into::into);
    let restart_needed = rollback_queued || staged.as_ref().is_some_and(|s| !s.download_only);
    Ok(UpdateStatus {
        installed, staged, restart_needed, rollback_queued,
        rollback_available: status.rollback.is_some() && !status.read_only,
        read_only: status.read_only,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn entry(version: Value) -> Value {
        json!({"image": {"image": {"image": "quay.io/example/arka:latest", "transport": "registry"},
            "version": version, "imageDigest": "sha256:123", "timestamp": null},
            "incompatible": false, "pinned": false, "ostree": {"checksum": "123"}})
    }
    fn host() -> Value {
        json!({"apiVersion": "org.containers.bootc/v1", "kind": "BootcHost",
            "spec": {"bootOrder": "default"},
            "status": {"booted": entry(json!("44.1")), "staged": null, "rollback": null}})
    }
    fn parse(value: Value) -> UpdateStatus { parse_status(&value.to_string()).unwrap() }
    #[test]
    fn published_snapshot_expires_and_rejects_future_timestamps() {
        let path = std::env::temp_dir().join(format!("arka-status-{}-{}.json", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path, host().to_string()).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert!(read_published_at(&path, modified).is_ok());
        assert!(read_published_at(&path, modified + MAX_AGE).is_err());
        assert!(read_published_at(&path, modified - std::time::Duration::from_secs(1)).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(read_published_at(&path, modified).is_err());
    }
    #[test]
    fn installed_version_is_metadata_not_tag_and_no_update_is_inferred() {
        let s = parse(host());
        assert_eq!(s.installed.version.as_deref(), Some("44.1"));
        assert!(!s.restart_needed);
        assert!(!s.rollback_available);
        assert!(s.staged.is_none());
    }
    #[test]
    fn staged_update_and_previous_deployment() {
        let mut v = host();
        v["status"]["staged"] = entry(json!("44.2"));
        v["status"]["rollback"] = entry(json!("44.0"));
        let s = parse(v);
        assert_eq!(s.staged.unwrap().version.as_deref(), Some("44.2"));
        assert!(s.restart_needed && s.rollback_available);
    }
    #[test]
    fn download_only_is_staged_but_not_ready_for_restart() {
        let mut v = host();
        v["status"]["staged"] = entry(json!("44.2"));
        v["status"]["staged"]["downloadOnly"] = json!(true);
        let s = parse(v);
        assert!(s.staged.unwrap().download_only);
        assert!(!s.restart_needed);
    }
    #[test]
    fn queued_rollback_requires_restart_without_staged_update() {
        for status_flag in [false, true] {
            let mut v = host();
            v["status"]["rollback"] = entry(json!("44.0"));
            if status_flag { v["status"]["rollbackQueued"] = json!(true); }
            else { v["spec"]["bootOrder"] = json!("rollback"); }
            let s = parse(v);
            assert!(s.rollback_queued && s.restart_needed);
        }
    }
    #[test]
    fn read_only_media_cannot_queue_rollback() {
        let mut v = host();
        v["status"]["rollback"] = entry(json!("44.0"));
        v["status"]["readOnly"] = json!(true);
        let s = parse(v);
        assert!(s.read_only);
        assert!(!s.rollback_available);
    }
    #[test]
    fn missing_version_and_cached_update_are_not_invented_status() {
        let mut v = host();
        v["status"]["booted"]["image"]["version"] = Value::Null;
        v["status"]["booted"]["cachedUpdate"] = entry(json!("99"))["image"].clone();
        let s = parse(v);
        assert!(s.installed.version.is_none());
        assert!(s.staged.is_none());
        assert!(!s.restart_needed);
    }
    #[test]
    fn malformed_unknown_or_unbooted_input_stays_unknown() {
        for bad in ["", "{}", "null", "{\"status\":{}}"] { assert!(parse_status(bad).is_err()); }
        for (path, value) in [("/apiVersion", json!("org.containers.bootc/v2")),
            ("/status/booted", Value::Null), ("/status/staged", json!({})),
            ("/status/staged", json!(false))] {
            let mut v = host();
            *v.pointer_mut(path).unwrap() = value;
            assert!(parse_status(&v.to_string()).is_err(), "{path}");
        }
        let mut v = host();
        v["status"]["rollbackQueued"] = json!(true);
        assert!(parse_status(&v.to_string()).is_err());
    }
    #[test]
    fn unknown_additional_fields_are_forward_compatible() {
        let mut v = host();
        v["status"]["futureField"] = json!({"anything": true});
        assert_eq!(parse(v), parse(host()));
    }
}
