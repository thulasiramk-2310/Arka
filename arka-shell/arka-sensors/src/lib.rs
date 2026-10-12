//! arka-sensors: who is using the microphone, camera or screen right now,
//! read from a PipeWire graph dump (`pw-dump`).
//!
//! What it claims: a capture is happening when a program's input stream has an
//! active link from a source. What it can name:
//!   * Verified: a native PipeWire client (PipeWire records the process ID
//!     itself, from the connection, so the program can be read from `/proc`),
//!     or a sandboxed app that carries a portal app ID.
//!   * Reported: a client of the PulseAudio compatibility layer. PipeWire records
//!     the compatibility server's own process ID for those, so the name is only
//!     what the app says about itself.
//! What it cannot see: programs that open a capture device directly, virtual
//! inputs that are not real hardware, and anything done by a compromised
//! kernel or root.

use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Microphone,
    Camera,
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Trust {
    Verified,
    Reported,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Use {
    pub kind: Kind,
    /// What to show: the program's name when verified, else what the app calls itself.
    pub name: String,
    pub trust: Trust,
    /// The name the app gave itself (may differ from `name`, and may be false).
    pub claimed: String,
    pub pid: Option<u32>,
}

/// Programs that relay other programs' audio. Their process ID says nothing
/// about who is really recording.
const RELAYS: [&str; 4] = ["pipewire-pulse", "pulseaudio", "pipewire", "wireplumber"];

fn props(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    v.get("info")?.get("props")?.as_object()
}

fn text<'a>(p: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    p.get(key)?.as_str().filter(|s| !s.is_empty())
}

fn number(p: &serde_json::Map<String, Value>, key: &str) -> Option<u32> {
    let v = p.get(key)?;
    v.as_u64().map(|n| n as u32).or_else(|| v.as_str()?.parse().ok())
}

fn basename(path: &str) -> String {
    let path = path.strip_suffix(" (deleted)").unwrap_or(path);
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// What kind of capture an active link into a program's input stream is, from
/// the node that produces the data. `None` means it is not a capture of
/// something private (for example a monitor of speaker output).
fn classify(source: &Value) -> Option<Kind> {
    let p = props(source)?;
    let class = text(p, "media.class")?;
    if class.starts_with("Audio/Source") {
        return Some(Kind::Microphone);
    }
    if class.starts_with("Stream/Output/Video") {
        // Another program is producing video for this one: a screen cast.
        return Some(Kind::Screen);
    }
    if class.starts_with("Video/Source") {
        let camera = text(p, "media.role") == Some("Camera")
            || matches!(text(p, "device.api"), Some("v4l2") | Some("libcamera"))
            || p.keys().any(|k| k.starts_with("api.v4l2.") || k.starts_with("api.libcamera."));
        return Some(if camera { Kind::Camera } else { Kind::Screen });
    }
    None
}

/// Every capture in `dump` (the JSON array printed by `pw-dump`). `exe_of`
/// returns the program a process ID belongs to (the target of `/proc/<pid>/exe`),
/// or `None` if it cannot be read.
pub fn find_uses(dump: &Value, exe_of: &dyn Fn(u32) -> Option<String>) -> Vec<Use> {
    let Some(objects) = dump.as_array() else { return Vec::new() };
    let mut nodes: HashMap<u64, &Value> = HashMap::new();
    let mut clients: HashMap<u64, &Value> = HashMap::new();
    for o in objects {
        let (Some(id), Some(kind)) = (o.get("id").and_then(Value::as_u64), o.get("type").and_then(Value::as_str)) else { continue };
        match kind {
            "PipeWire:Interface:Node" => {
                nodes.insert(id, o);
            }
            "PipeWire:Interface:Client" => {
                clients.insert(id, o);
            }
            _ => {}
        }
    }

    let mut found = BTreeSet::new();
    for link in objects.iter().filter(|o| o.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Link")) {
        let Some(info) = link.get("info") else { continue };
        if info.get("state").and_then(Value::as_str) != Some("active") {
            continue; // negotiated but no data flowing
        }
        let (Some(from), Some(to)) = (
            info.get("output-node-id").and_then(Value::as_u64).and_then(|i| nodes.get(&i)),
            info.get("input-node-id").and_then(Value::as_u64).and_then(|i| nodes.get(&i)),
        ) else {
            continue;
        };
        let Some(consumer) = props(to) else { continue };
        if !text(consumer, "media.class").is_some_and(|c| c.starts_with("Stream/Input/")) {
            continue; // not a program reading a stream
        }
        let Some(kind) = classify(from) else { continue };

        let client = consumer.get("client.id").and_then(Value::as_u64).and_then(|i| clients.get(&i)).and_then(|c| props(c));
        let claimed = text(consumer, "application.name")
            .or_else(|| client.and_then(|c| text(c, "application.name")))
            .or_else(|| text(consumer, "node.name"))
            .unwrap_or("unknown")
            .to_string();

        let portal = client.and_then(|c| text(c, "pipewire.access.portal.app_id"));
        let secure_pid = client.and_then(|c| number(c, "pipewire.sec.pid"));
        let exe = secure_pid.and_then(exe_of);

        let (name, trust, pid) = if let Some(app) = portal {
            (app.to_string(), Trust::Verified, secure_pid)
        } else if let (Some(exe), Some(pid)) = (exe.as_deref(), secure_pid) {
            let program = basename(exe);
            if RELAYS.contains(&program.as_str()) {
                let reported = consumer.get("application.process.id").and_then(|v| v.as_u64()).map(|n| n as u32);
                (claimed.clone(), Trust::Reported, reported)
            } else {
                (program, Trust::Verified, Some(pid))
            }
        } else {
            (claimed.clone(), Trust::Reported, None)
        };
        found.insert(Use { kind, name, trust, claimed, pid });
    }
    found.into_iter().collect()
}

/// Splits the output of `pw-dump --monitor` into objects and says which ones
/// could change who is capturing. That output is one JSON array that stays open;
/// each change is one pretty-printed object that starts at a line "  {" and ends
/// at a line "  }" (or "  },").
///
/// Why filter: every time the overlay re-reads the graph, `pw-dump` itself joins
/// PipeWire as a client, and the monitor reports that client coming and going.
/// Reacting to every change made the overlay re-read about four times a second
/// and use a few percent of a core while nothing was happening.
#[derive(Default)]
pub struct EventFilter {
    inside: bool,
    text: String,
}

impl EventFilter {
    /// Feed one line; returns true when a relevant object has just ended.
    pub fn push_line(&mut self, line: &str) -> bool {
        if !self.inside {
            if line == "  {" {
                self.inside = true;
                self.text.clear();
            }
            return false;
        }
        if line == "  }" || line == "  }," {
            self.inside = false;
            return relevant(&self.text);
        }
        self.text.push_str(line);
        self.text.push('\n');
        false
    }
}

/// A link changing, or a program stream, source or screen-cast node changing or
/// going away. Other objects (clients, ports, modules, devices) cannot change
/// the answer by themselves.
fn relevant(object: &str) -> bool {
    if object.contains("\"type\": \"PipeWire:Interface:Link\"") {
        return true;
    }
    if !object.contains("\"type\": \"PipeWire:Interface:Node\"") {
        return false;
    }
    object.contains("\"info\": null")
        || ["Stream/Input", "Audio/Source", "Video/Source", "Stream/Output/Video"]
            .iter()
            .any(|class| object.contains(&format!("\"media.class\": \"{class}")))
}

/// One line for a tooltip or a screen reader, e.g. "Microphone: firefox (reported by the app)".
pub fn describe(kind: Kind, uses: &[Use]) -> String {
    let label = match kind {
        Kind::Microphone => "Microphone",
        Kind::Camera => "Camera",
        Kind::Screen => "Screen capture",
    };
    let list: Vec<String> = uses
        .iter()
        .filter(|u| u.kind == kind)
        .map(|u| match u.trust {
            Trust::Verified => u.name.clone(),
            Trust::Reported => format!("{} (reported by the app)", u.name),
        })
        .collect();
    format!("{label}: {}", list.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: u64, class: &str, extra: Value) -> Value {
        let mut p = json!({"media.class": class});
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        json!({"id": id, "type": "PipeWire:Interface:Node", "info": {"state": "running", "props": p}})
    }
    fn client(id: u64, extra: Value) -> Value {
        json!({"id": id, "type": "PipeWire:Interface:Client", "info": {"props": extra}})
    }
    fn link(id: u64, from: u64, to: u64, state: &str) -> Value {
        json!({"id": id, "type": "PipeWire:Interface:Link", "info": {"state": state, "output-node-id": from, "input-node-id": to}})
    }
    fn exes(pid: u32) -> Option<String> {
        match pid {
            100 => Some("/usr/bin/arka-test-recorder".into()),
            200 => Some("/usr/bin/pipewire-pulse".into()),
            300 => Some("/usr/bin/evil (deleted)".into()),
            _ => None,
        }
    }

    // Shape taken from a real capture on a laptop: a recorder linked to the
    // microphone, with two links (left and right channel).
    fn mic_graph(client_props: Value, app_name: &str) -> Value {
        json!([
            node(56, "Audio/Source", json!({"node.name": "alsa_input.pci", "device.api": "alsa"})),
            node(84, "Stream/Input/Audio", json!({"client.id": 83, "application.name": app_name})),
            client(83, client_props),
            link(77, 56, 84, "active"),
            link(78, 56, 84, "active")
        ])
    }

    #[test]
    fn native_client_is_verified_by_its_process() {
        let g = mic_graph(json!({"pipewire.sec.pid": 100, "application.name": "recorder"}), "recorder");
        let uses = find_uses(&g, &exes);
        assert_eq!(uses.len(), 1, "two stereo links are one use");
        assert_eq!(uses[0].kind, Kind::Microphone);
        assert_eq!(uses[0].trust, Trust::Verified);
        assert_eq!(uses[0].name, "arka-test-recorder");
        assert_eq!(uses[0].pid, Some(100));
    }

    #[test]
    fn a_program_that_lies_about_its_name_is_shown_as_what_it_is() {
        let g = mic_graph(json!({"pipewire.sec.pid": 300}), "Firefox");
        let uses = find_uses(&g, &exes);
        assert_eq!(uses[0].name, "evil", "the real program, not the claimed name");
        assert_eq!(uses[0].claimed, "Firefox");
        assert_eq!(uses[0].trust, Trust::Verified);
    }

    #[test]
    fn pulse_layer_clients_are_only_reported() {
        // The process ID PipeWire records is the compatibility server's own.
        let mut g = mic_graph(json!({"pipewire.sec.pid": 200}), "Firefox");
        g[1]["info"]["props"]["application.process.id"] = json!(4321);
        let uses = find_uses(&g, &exes);
        assert_eq!(uses[0].trust, Trust::Reported);
        assert_eq!(uses[0].name, "Firefox");
        assert_eq!(uses[0].pid, Some(4321));
        assert_eq!(describe(Kind::Microphone, &uses), "Microphone: Firefox (reported by the app)");
    }

    #[test]
    fn a_sandboxed_app_is_named_by_its_portal_id() {
        let g = mic_graph(json!({"pipewire.sec.pid": 200, "pipewire.access.portal.app_id": "org.example.Call"}), "x");
        let uses = find_uses(&g, &exes);
        assert_eq!(uses[0].name, "org.example.Call");
        assert_eq!(uses[0].trust, Trust::Verified);
    }

    #[test]
    fn an_unreadable_process_falls_back_to_reported() {
        let g = mic_graph(json!({"pipewire.sec.pid": 999}), "Someone");
        let uses = find_uses(&g, &exes);
        assert_eq!(uses[0].trust, Trust::Reported);
        assert_eq!(uses[0].name, "Someone");
    }

    #[test]
    fn a_camera_is_recognised_by_its_role_or_driver() {
        for extra in [json!({"media.role": "Camera"}), json!({"device.api": "v4l2"}), json!({"api.v4l2.path": "/dev/video0"})] {
            let g = json!([
                node(66, "Video/Source", extra),
                node(90, "Stream/Input/Video", json!({"client.id": 91, "application.name": "meet"})),
                client(91, json!({"pipewire.sec.pid": 100})),
                link(1, 66, 90, "active")
            ]);
            assert_eq!(find_uses(&g, &exes)[0].kind, Kind::Camera);
        }
    }

    #[test]
    fn a_screen_cast_is_a_video_stream_made_by_another_program() {
        let g = json!([
            node(70, "Stream/Output/Video", json!({"client.id": 71, "application.name": "kwin_wayland"})),
            node(90, "Stream/Input/Video", json!({"client.id": 91, "application.name": "meet"})),
            client(91, json!({"pipewire.sec.pid": 100})),
            link(1, 70, 90, "active")
        ]);
        let uses = find_uses(&g, &exes);
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].kind, Kind::Screen);
    }

    #[test]
    fn a_video_source_that_is_not_a_camera_counts_as_the_screen() {
        let g = json!([
            node(70, "Video/Source", json!({"node.name": "screen-cast"})),
            node(90, "Stream/Input/Video", json!({"client.id": 91})),
            client(91, json!({"pipewire.sec.pid": 100})),
            link(1, 70, 90, "active")
        ]);
        assert_eq!(find_uses(&g, &exes)[0].kind, Kind::Screen);
    }

    #[test]
    fn a_link_with_no_data_flowing_is_not_a_use() {
        for state in ["paused", "init", "negotiating"] {
            let mut g = mic_graph(json!({"pipewire.sec.pid": 100}), "x");
            g[3]["info"]["state"] = json!(state);
            g[4]["info"]["state"] = json!(state);
            assert!(find_uses(&g, &exes).is_empty(), "{state}");
        }
    }

    #[test]
    fn recording_the_speaker_output_is_not_a_microphone() {
        let g = json!([
            node(50, "Audio/Sink", json!({"node.name": "speakers"})),
            node(84, "Stream/Input/Audio", json!({"client.id": 83})),
            client(83, json!({"pipewire.sec.pid": 100})),
            link(1, 50, 84, "active")
        ]);
        assert!(find_uses(&g, &exes).is_empty());
    }

    #[test]
    fn playback_is_not_a_capture() {
        let g = json!([
            node(56, "Audio/Source", json!({})),
            node(85, "Stream/Output/Audio", json!({"client.id": 83})),
            client(83, json!({"pipewire.sec.pid": 100})),
            link(1, 56, 85, "active")
        ]);
        assert!(find_uses(&g, &exes).is_empty());
    }

    #[test]
    fn an_empty_or_odd_graph_gives_nothing() {
        assert!(find_uses(&json!([]), &exes).is_empty());
        assert!(find_uses(&json!({"not": "an array"}), &exes).is_empty());
        assert!(find_uses(&json!([{"id": 1}]), &exes).is_empty());
    }

    fn feed(lines: &[&str]) -> Vec<bool> {
        let mut f = EventFilter::default();
        lines.iter().map(|l| f.push_line(l)).collect()
    }

    #[test]
    fn a_client_coming_or_going_does_not_trigger_a_re_read() {
        let r = feed(&["[", "  {", "    \"id\": 57,", "    \"type\": \"PipeWire:Interface:Client\",", "    \"info\": null", "  },"]);
        assert!(!r.iter().any(|x| *x));
    }

    #[test]
    fn a_link_change_triggers_a_re_read() {
        let r = feed(&["  {", "    \"id\": 77,", "    \"type\": \"PipeWire:Interface:Link\",", "    \"info\": {}", "  },"]);
        assert_eq!(r.last(), Some(&true));
    }

    #[test]
    fn a_program_stream_or_source_triggers_but_playback_and_sinks_do_not() {
        let node = |class: &str| feed(&["  {", "    \"type\": \"PipeWire:Interface:Node\",", &format!("        \"media.class\": \"{class}\","), "  }"]);
        for c in ["Stream/Input/Audio", "Audio/Source", "Audio/Source/Virtual", "Video/Source", "Stream/Input/Video", "Stream/Output/Video"] {
            assert_eq!(node(c).last(), Some(&true), "{c}");
        }
        for c in ["Stream/Output/Audio", "Audio/Sink"] {
            assert_eq!(node(c).last(), Some(&false), "{c}");
        }
    }

    #[test]
    fn a_node_going_away_triggers_a_re_read() {
        let r = feed(&["  {", "    \"id\": 84,", "    \"type\": \"PipeWire:Interface:Node\",", "    \"info\": null", "  }"]);
        assert_eq!(r.last(), Some(&true));
    }

    #[test]
    fn the_filter_survives_odd_input() {
        let r = feed(&["", "garbage", "  }", "  {", "  }", "  {", "    \"type\": \"PipeWire:Interface:Link\"", "  },"]);
        assert_eq!(r.last(), Some(&true));
        assert_eq!(r.iter().filter(|x| **x).count(), 1);
    }

    #[test]
    fn the_description_lists_each_program_once_per_kind() {
        let uses = vec![
            Use { kind: Kind::Microphone, name: "a".into(), trust: Trust::Verified, claimed: "a".into(), pid: Some(1) },
            Use { kind: Kind::Microphone, name: "b".into(), trust: Trust::Reported, claimed: "b".into(), pid: None },
            Use { kind: Kind::Camera, name: "c".into(), trust: Trust::Verified, claimed: "c".into(), pid: Some(3) },
        ];
        assert_eq!(describe(Kind::Microphone, &uses), "Microphone: a, b (reported by the app)");
        assert_eq!(describe(Kind::Camera, &uses), "Camera: c");
    }
}
