//! X11 apps inside ArkaWM, through xwayland-satellite.
//!
//! xwayland-satellite is a separate program: it starts Xwayland, acts as the X11 window manager and
//! shows each X11 window to ArkaWM as an ordinary Wayland window. ArkaWM opens the X11 socket
//! itself, points `DISPLAY` at it for the programs it launches, and starts xwayland-satellite only
//! when the first X11 app connects, handing it the open socket (`-listenfd`). Until then no X
//! server runs: Xwayland and the helper took about 130 MB together on the development laptop,
//! which a computer with little memory should not spend on X11 apps it may never open.

use std::{
    os::{
        fd::{AsFd, AsRawFd},
        linux::net::SocketAddrExt,
        unix::{
            net::{SocketAddr, UnixListener},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use smithay::reexports::calloop::{generic::Generic, Interest, LoopHandle, Mode, PostAction};

use crate::state::ArkaWm;

/// The helper program; `ARKAWM_XWAYLAND` names another path, or `off` turns X11 support off.
const HELPER: &str = "xwayland-satellite";

/// This session's X11 display: its sockets, and the helper once an app has asked for it. Dropping
/// it stops the helper and removes the socket and lock files.
pub struct XWayland {
    number: u32,
    child: std::rc::Rc<std::cell::RefCell<Option<Child>>>,
}

/// The first X11 display number nobody is using: no socket and no lock file.
pub fn free_display(taken: impl Fn(u32) -> bool) -> Option<u32> {
    (0..64).find(|&n| !taken(n))
}

/// Whether display `n` is in use. A lock file left by a process that is gone (an ArkaWM that was
/// killed and could not clean up) is cleared away with its socket, as X servers do.
fn display_taken(n: u32) -> bool {
    if let Ok(text) = std::fs::read_to_string(lock_path(n)) {
        let gone = text.trim().parse::<u32>().is_ok_and(|pid| !Path::new(&format!("/proc/{pid}")).exists());
        if !gone {
            return true;
        }
        let _ = std::fs::remove_file(lock_path(n));
        let _ = std::fs::remove_file(socket_path(n));
    }
    socket_path(n).exists() || lock_path(n).exists()
}

fn socket_path(n: u32) -> PathBuf {
    PathBuf::from(format!("/tmp/.X11-unix/X{n}"))
}

fn lock_path(n: u32) -> PathBuf {
    PathBuf::from(format!("/tmp/.X{n}-lock"))
}

/// Claim display `n`: the lock file (as X servers write it: the process id in 10 characters),
/// then the socket in /tmp/.X11-unix and the abstract one X11 programs on Linux try first.
fn claim(n: u32) -> std::io::Result<(UnixListener, UnixListener)> {
    use std::io::Write;
    let mut lock = std::fs::OpenOptions::new().write(true).create_new(true).open(lock_path(n))?;
    write!(lock, "{:>10}\n", std::process::id())?;
    let _ = std::fs::create_dir_all("/tmp/.X11-unix");
    let bound = (|| {
        let file = UnixListener::bind(socket_path(n))?;
        let abstract_name = SocketAddr::from_abstract_name(format!("/tmp/.X11-unix/X{n}"))?;
        let hidden = UnixListener::bind_addr(&abstract_name)?;
        Ok((file, hidden))
    })();
    if bound.is_err() {
        let _ = std::fs::remove_file(socket_path(n));
        let _ = std::fs::remove_file(lock_path(n));
    }
    bound
}

/// Open the X11 display and set `DISPLAY` for programs started later; xwayland-satellite starts
/// when the first X11 app connects.
///
/// `DISPLAY` is cleared first, so an X11 app can never land on the X server of the session ArkaWM
/// itself runs in. Returns `None` (and X11 apps simply fail to start) if X11 support is off or no
/// display could be opened.
pub fn start(handle: &LoopHandle<'static, ArkaWm>, wayland_display: &std::ffi::OsStr) -> Option<XWayland> {
    std::env::remove_var("DISPLAY");
    let helper = std::env::var("ARKAWM_XWAYLAND").unwrap_or_else(|_| HELPER.to_string());
    if helper == "off" {
        return None;
    }
    let number = free_display(display_taken)?;
    let (file, hidden) = match claim(number) {
        Ok(sockets) => sockets,
        Err(err) => {
            tracing::warn!("X11 apps will not run: could not open display :{number}: {err}");
            return None;
        }
    };
    let child: std::rc::Rc<std::cell::RefCell<Option<Child>>> = Default::default();
    let wayland_display = wayland_display.to_owned();
    let started = child.clone();
    // Readable (either socket) means an X11 app is connecting. The helper takes both sockets
    // over; ArkaWM stops listening and keeps nothing open.
    let watched: Vec<_> = [file.as_fd().try_clone_to_owned(), hidden.as_fd().try_clone_to_owned()].into_iter().collect::<Result<_, _>>().ok()?;
    let sockets = std::rc::Rc::new(std::cell::RefCell::new(Some((file, hidden))));
    let launch = move || {
        let Some((file, hidden)) = sockets.borrow_mut().take() else { return };
        let fds = [file.as_raw_fd(), hidden.as_raw_fd()];
        let mut command = Command::new(&helper);
        command
            .arg(format!(":{number}"))
            .args(["-listenfd", &fds[0].to_string(), "-listenfd", &fds[1].to_string()])
            .env("WAYLAND_DISPLAY", &wayland_display)
            .stdin(Stdio::null());
        // SAFETY: only clears the close-on-exec flag of two descriptors this process owns, between
        // fork and exec; no allocation, no locks.
        unsafe {
            command.pre_exec(move || {
                for fd in fds {
                    let fd = std::os::fd::BorrowedFd::borrow_raw(fd);
                    smithay::reexports::rustix::io::fcntl_setfd(fd, smithay::reexports::rustix::io::FdFlags::empty())?;
                }
                Ok(())
            });
        }
        match command.spawn() {
            Ok(child) => {
                tracing::info!("an X11 app asked for :{number}; started {helper}");
                *started.borrow_mut() = Some(child);
            }
            Err(err) => tracing::warn!("X11 apps will not run: could not start {helper}: {err}"),
        }
        // our copies close here; the helper has its own
        drop((file, hidden));
    };
    let launch = std::rc::Rc::new(launch);
    for fd in watched {
        let launch = launch.clone();
        let inserted = handle.insert_source(Generic::new(fd, Interest::READ, Mode::Level), move |_, _, _| {
            launch();
            Ok(PostAction::Remove)
        });
        if let Err(err) = inserted {
            tracing::warn!("X11 apps will not run: {err}");
            let _ = std::fs::remove_file(socket_path(number));
            let _ = std::fs::remove_file(lock_path(number));
            return None;
        }
    }
    std::env::set_var("DISPLAY", format!(":{number}"));
    tracing::info!("X11 apps run on DISPLAY=:{number}; the X server starts with the first one");
    Some(XWayland { number, child })
}

impl Drop for XWayland {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.borrow_mut().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(socket_path(self.number));
        let _ = std::fs::remove_file(lock_path(self.number));
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
