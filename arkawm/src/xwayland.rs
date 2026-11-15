//! X11 apps inside ArkaWM, through xwayland-satellite.
//!
//! xwayland-satellite is a separate program: it starts Xwayland, acts as the X11 window manager and
//! shows each X11 window to ArkaWM as an ordinary Wayland window. ArkaWM only has to start it, point
//! `DISPLAY` at it for the programs it launches, and stop it on exit.

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// The helper program; `ARKAWM_XWAYLAND` names another path, or `off` turns X11 support off.
const HELPER: &str = "xwayland-satellite";
/// How long to wait for the X11 socket before giving up.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// A running X11 server for this session. Dropping it stops the server.
pub struct XWayland {
    child: Child,
}

/// The first X11 display number nobody is using: no socket and no lock file.
pub fn free_display(taken: impl Fn(u32) -> bool) -> Option<u32> {
    (0..64).find(|&n| !taken(n))
}

fn display_taken(n: u32) -> bool {
    Path::new(&format!("/tmp/.X11-unix/X{n}")).exists() || Path::new(&format!("/tmp/.X{n}-lock")).exists()
}

/// Start the X11 server on our Wayland socket and set `DISPLAY` for programs started later.
///
/// `DISPLAY` is cleared first, so an X11 app can never land on the X server of the session ArkaWM
/// itself runs in. Returns `None` (and X11 apps simply fail to start) if the helper is missing or
/// does not come up in time.
pub fn start(wayland_display: &std::ffi::OsStr) -> Option<XWayland> {
    std::env::remove_var("DISPLAY");

    let helper = std::env::var("ARKAWM_XWAYLAND").unwrap_or_else(|_| HELPER.to_string());
    if helper == "off" {
        return None;
    }
    let number = free_display(display_taken)?;
    let name = format!(":{number}");

    let child = Command::new(&helper)
        .arg(&name)
        .env("WAYLAND_DISPLAY", wayland_display)
        .stdin(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            tracing::warn!("X11 apps will not run: could not start {helper}: {err}");
            return None;
        }
    };

    // The helper creates the socket before it needs anything from us, so waiting here is safe.
    let socket = format!("/tmp/.X11-unix/X{number}");
    let begun = Instant::now();
    while !Path::new(&socket).exists() {
        if let Ok(Some(status)) = child.try_wait() {
            tracing::warn!("X11 apps will not run: {helper} stopped at once ({status})");
            return None;
        }
        if begun.elapsed() > READY_TIMEOUT {
            tracing::warn!("X11 apps will not run: {helper} did not open {socket} in time");
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    std::env::set_var("DISPLAY", &name);
    tracing::info!("X11 apps run on DISPLAY={name}");
    Some(XWayland { child })
}

impl Drop for XWayland {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::free_display;

    #[test]
    fn picks_the_first_display_nobody_uses() {
        assert_eq!(free_display(|n| n < 2), Some(2));
        assert_eq!(free_display(|_| false), Some(0));
        assert_eq!(free_display(|_| true), None);
    }
}
