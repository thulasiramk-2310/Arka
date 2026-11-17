//! arka-desk never draws through X11, but while GTK sets up its renderer some graphics drivers
//! open the X display anyway (seen with NVIDIA's Vulkan driver and with Mesa's software GL in a
//! VM). That started the X server at every login with no X11 app open: 60 to 130 MB measured. So
//! arka-desk forgets DISPLAY for itself when it starts and gives it back to the programs it
//! starts, which may be X11 apps.

use std::ffi::OsString;
use std::process::Command;
use std::sync::OnceLock;

use gtk4::gio;
use gtk4::prelude::*;

static DISPLAY: OnceLock<Option<OsString>> = OnceLock::new();

/// First thing in main, before any thread or GTK starts.
pub fn hide() {
    let display = std::env::var_os("DISPLAY");
    std::env::remove_var("DISPLAY");
    let _ = DISPLAY.set(display);
}

fn display() -> Option<&'static OsString> {
    DISPLAY.get().and_then(Option::as_ref)
}

/// `sh -c line` for starting a program, with DISPLAY given back.
pub fn shell(line: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(line);
    give_back_to(&mut command);
    command
}

/// DISPLAY for one program arka-desk starts (a fresh copy of itself too, which hides it again).
pub fn give_back_to(command: &mut Command) {
    if let Some(display) = display() {
        command.env("DISPLAY", display);
    }
}

/// The same for an app started through GIO.
pub fn give_back(context: &impl IsA<gio::AppLaunchContext>) {
    if let Some(display) = display() {
        context.setenv("DISPLAY", display);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn programs_get_the_display_back() {
        std::env::set_var("DISPLAY", ":5");
        super::hide();
        assert!(std::env::var_os("DISPLAY").is_none());
        let command = super::shell("true");
        let given: Vec<_> = command.get_envs().filter(|(key, _)| *key == "DISPLAY").collect();
        assert_eq!(given, [(std::ffi::OsStr::new("DISPLAY"), Some(std::ffi::OsStr::new(":5")))]);
    }
}
