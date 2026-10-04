use std::process::Command;

pub const SINK: &str = "@DEFAULT_AUDIO_SINK@";
pub const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume {
    pub level: f64,
    pub muted: bool,
}

#[derive(Debug)]
pub struct Device {
    pub volume: Volume,
    pub name: String,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Refresh,
    Volume(f64),
    MuteOutput(bool),
    MuteInput(bool),
}

pub struct Snapshot {
    pub output: Result<Device, String>,
    pub input: Result<Device, String>,
    pub error: Option<String>,
}

fn parse_volume(text: &str) -> Result<Volume, String> {
    let fields: Vec<_> = text.split_whitespace().collect();
    if !(fields.len() == 2 || fields.len() == 3 && fields[2] == "[MUTED]") || fields[0] != "Volume:"
    {
        return Err("Unrecognized audio state".into());
    }
    let level = fields[1].parse::<f64>().map_err(|_| "Invalid volume")?;
    if !level.is_finite() || level < 0.0 {
        return Err("Invalid volume".into());
    }
    Ok(Volume {
        level,
        muted: fields.len() == 3,
    })
}

fn device(
    id: &str,
    run: &mut impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<Device, String> {
    // Query errors never become an invented volume or mute state.
    let volume = parse_volume(&run(&["get-volume", id])?)?;
    let details = run(&["inspect", id]).unwrap_or_default();
    let name = details
        .lines()
        .find_map(|line| {
            let (key, value) = line.trim().trim_start_matches('*').trim().split_once('=')?;
            matches!(key.trim(), "node.description" | "node.nick")
                .then(|| value.trim().trim_matches('"').to_owned())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| "Device name unavailable".into());
    Ok(Device { volume, name })
}

fn refresh_with(
    action: Action,
    mut run: impl FnMut(&[&str]) -> Result<String, String>,
) -> Snapshot {
    let result = match action {
        Action::Refresh => Ok(String::new()),
        Action::Volume(value) if value.is_finite() && (0.0..=1.0).contains(&value) => {
            run(&["set-volume", SINK, &format!("{value:.2}")])
        }
        Action::Volume(_) => Err("Invalid requested volume".into()),
        Action::MuteOutput(muted) => run(&["set-mute", SINK, if muted { "1" } else { "0" }]),
        Action::MuteInput(muted) => run(&["set-mute", SOURCE, if muted { "1" } else { "0" }]),
    };
    // Wait for the write to finish before observing its result, including failed writes.
    let output = device(SINK, &mut run);
    let input = device(SOURCE, &mut run);
    let confirmed = match action {
        Action::Refresh => true,
        Action::Volume(value) => output
            .as_ref()
            .is_ok_and(|d| (d.volume.level - value).abs() < 0.011),
        Action::MuteOutput(muted) => output.as_ref().is_ok_and(|d| d.volume.muted == muted),
        Action::MuteInput(muted) => input.as_ref().is_ok_and(|d| d.volume.muted == muted),
    };
    let error = result
        .err()
        .or_else(|| (!confirmed).then(|| "Audio change could not be confirmed".into()));
    Snapshot {
        output,
        input,
        error,
    }
}

pub fn refresh(action: Action) -> Snapshot {
    refresh_with(action, |args| {
        let output = Command::new("timeout")
            .args(["--kill-after=1s", "3s", "wpctl"])
            .args(args)
            .env("LC_ALL", "C")
            .output()
            .map_err(|e| format!("Could not query audio: {e}"))?;
        if !output.status.success() {
            return Err(format!("Audio command failed ({})", output.status));
        }
        String::from_utf8(output.stdout).map_err(|_| "Invalid audio response".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_actual_volume_and_mute_without_defaults() {
        assert_eq!(
            parse_volume("Volume: 0.65 [MUTED]\n"),
            Ok(Volume {
                level: 0.65,
                muted: true
            })
        );
        assert_eq!(
            parse_volume("Volume: 1.25\n"),
            Ok(Volume {
                level: 1.25,
                muted: false
            })
        );
        for text in [
            "",
            "error",
            "Volume: NaN",
            "Volume: inf",
            "Volume: -1",
            "Volume: 1 [UNKNOWN]",
            "Volume: 1 extra extra",
        ] {
            assert!(parse_volume(text).is_err(), "{text}");
        }
    }

    #[test]
    fn waits_for_write_and_reads_back_both_devices() {
        let mut calls = Vec::new();
        let state = refresh_with(Action::MuteInput(true), |args| {
            calls.push(args.join(" "));
            Ok(if args[0] == "get-volume" && args[1] == SOURCE {
                "Volume: 1 [MUTED]"
            } else {
                "Volume: 0.5"
            }
            .into())
        });
        assert_eq!(calls[0], format!("set-mute {SOURCE} 1"));
        assert_eq!(calls[1], format!("get-volume {SINK}"));
        assert!(state.input.unwrap().volume.muted);
        assert!(state.error.is_none());
    }

    #[test]
    fn failed_write_is_reported_even_if_readback_matches() {
        let state = refresh_with(Action::MuteInput(true), |args| {
            if args[0] == "set-mute" {
                Err("timed out".into())
            } else {
                Ok("Volume: 1 [MUTED]".into())
            }
        });
        assert_eq!(state.error.as_deref(), Some("timed out"));
    }

    #[test]
    fn successful_exit_is_not_proof_of_mute() {
        let state = refresh_with(Action::MuteInput(true), |_| Ok("Volume: 1".into()));
        assert!(state.error.is_some());
        assert!(!state.input.unwrap().volume.muted);
    }

    #[test]
    fn missing_output_does_not_hide_working_microphone() {
        let state = refresh_with(Action::Refresh, |args| {
            if args[1] == SINK {
                Err("no output device".into())
            } else {
                Ok("Volume: 0.8 [MUTED]".into())
            }
        });
        assert!(state.output.is_err());
        assert!(state.input.unwrap().volume.muted);
    }

    #[test]
    fn unavailable_service_leaves_both_states_unknown() {
        let state = refresh_with(Action::Refresh, |_| Err("offline".into()));
        assert!(state.output.is_err());
        assert!(state.input.is_err());
    }
}
