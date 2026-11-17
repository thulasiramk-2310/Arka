//! Developer test hook: scripted key presses through the real keyboard path.
//!
//! `ARKAWM_TEST_KEYS="3000 29+57 35 37 36 28"` waits 3000 ms, then presses each group in turn
//! (numbers are Linux key codes; `29+57` is Ctrl+Space held together), 150 ms apart. `m640,400`
//! moves the pointer there, `left` and `right` click, `shot:/tmp/a.ppm` saves the screen, `d56`/`u56` hold a key down and let it go,
//! and `w1500` waits 1500 ms before the next step (a bare number later in the script is a key). Unlike a
//! virtual keyboard, these keys go through shortcuts, menus and the input method, exactly like the
//! real keyboard. Only compiled into debug builds.

use std::time::Duration;

use smithay::{
    backend::input::{ButtonState, KeyState},
    reexports::calloop::{
        timer::{TimeoutAction, Timer},
        LoopHandle,
    },
};

use crate::ArkaWm;

/// A start delay in milliseconds, then groups of key codes pressed together.
pub fn parse(script: &str) -> Option<(u64, Vec<Step>)> {
    let mut words = script.split_whitespace();
    let delay = words.next()?.parse().ok()?;
    let steps = words.map(parse_step).collect::<Option<Vec<_>>>()?;
    Some((delay, steps))
}

#[derive(Debug, PartialEq)]
pub enum Step {
    /// Keys pressed together, as Linux key codes.
    Keys(Vec<u32>),
    /// Move the pointer to this point, in logical pixels: `m640,400`.
    Move(f64, f64),
    /// Click: `left` or `right`.
    Click(u32),
    /// Hold (`press`) or let go of (`release`) the left button, for drags.
    Hold(bool),
    /// Hold a key down (`d56`) or let it go (`u56`), for shortcuts that act on release.
    Key(u32, bool),
    /// Save the screen to a PPM file: `shot:/tmp/a.ppm`.
    Shot(String),
    /// Wait this long before the next step: `w1500`.
    Wait(u64),
}

fn parse_step(word: &str) -> Option<Step> {
    if let Some(path) = word.strip_prefix("shot:") {
        return Some(Step::Shot(path.to_string()));
    }
    if let Some(ms) = word.strip_prefix('w').and_then(|ms| ms.parse().ok()) {
        return Some(Step::Wait(ms));
    }
    if let Some(point) = word.strip_prefix('m') {
        let (x, y) = point.split_once(',')?;
        return Some(Step::Move(x.parse().ok()?, y.parse().ok()?));
    }
    for (prefix, down) in [("d", true), ("u", false)] {
        if let Some(code) = word.strip_prefix(prefix).and_then(|code| code.parse().ok()) {
            return Some(Step::Key(code, down));
        }
    }
    match word {
        "left" => Some(Step::Click(0x110)),
        "right" => Some(Step::Click(0x111)),
        "press" => Some(Step::Hold(true)),
        "release" => Some(Step::Hold(false)),
        _ => word.split('+').map(|code| code.parse().ok()).collect::<Option<Vec<u32>>>().map(Step::Keys),
    }
}

pub fn install(handle: &LoopHandle<'static, ArkaWm>) {
    let Ok(script) = std::env::var("ARKAWM_TEST_KEYS") else { return };
    let Some((delay, groups)) = parse(&script) else {
        tracing::warn!("ARKAWM_TEST_KEYS not understood: {script}");
        return;
    };
    let mut groups = groups.into_iter();
    let timer = Timer::from_duration(Duration::from_millis(delay));
    let _ = handle.insert_source(timer, move |_, _, state| {
        let Some(step) = groups.next() else { return TimeoutAction::Drop };
        tracing::info!("test step {step:?}");
        state.redraw_wanted = true;
        let time = state.start_time.elapsed().as_millis() as u32;
        if let Step::Wait(ms) = step {
            return TimeoutAction::ToDuration(Duration::from_millis(ms));
        }
        match step {
            Step::Wait(_) => {}
            Step::Keys(group) => {
                // evdev codes are 8 below XKB key codes
                for &code in &group {
                    state.key((code + 8).into(), KeyState::Pressed, time);
                }
                for &code in group.iter().rev() {
                    state.key((code + 8).into(), KeyState::Released, time + 20);
                }
            }
            Step::Key(code, down) => {
                state.key((code + 8).into(), if down { KeyState::Pressed } else { KeyState::Released }, time);
            }
            Step::Hold(down) => {
                let button_state = if down { ButtonState::Pressed } else { ButtonState::Released };
                state.pointer_button(0x110, button_state, time);
            }
            Step::Shot(path) => state.shot_request = Some(path.into()),
            Step::Move(x, y) => state.pointer_move((x, y).into(), time),
            Step::Click(button) => {
                state.pointer_button(button, ButtonState::Pressed, time);
                state.pointer_button(button, ButtonState::Released, time + 20);
            }
        }
        TimeoutAction::ToDuration(Duration::from_millis(150))
    });
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_a_key_script() {
        use super::Step::*;
        assert_eq!(parse("3000 29+57 35"), Some((3000, vec![Keys(vec![29, 57]), Keys(vec![35])])));
        assert_eq!(parse("10 m640,400 right left"), Some((10, vec![Move(640.0, 400.0), Click(0x111), Click(0x110)])));
        assert_eq!(parse("10 m640"), None);
        assert_eq!(parse("500"), Some((500, vec![])));
        assert_eq!(parse("10 w1500 left"), Some((10, vec![Wait(1500), Click(0x110)])));
        assert_eq!(parse(""), None);
        assert_eq!(parse("soon 35"), None);
        assert_eq!(parse("100 a+b"), None);
    }
}
