//! The link between ArkaWM and its desktop shell: a socket the shell reads the list of open
//! windows from and sends simple commands to (go to a window, minimise it, show the desktop).
//!
//! One line per message. The shell sends `watch` once and from then on receives a `windows` line
//! whenever the list changes, and `show <panel>` / `shot <file>` lines when a shortcut asks the
//! shell for something. Fields are separated by tabs, windows by the unit separator (0x1F).
//!
//! The socket is in the user's runtime folder, readable by that user only. Any program of that
//! user can use it, the same as on other desktops; keeping an untrusted app away from it is the
//! job of that app's sandbox.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};

use smithay::reexports::calloop::{generic::Generic, Interest, LoopHandle, Mode, PostAction};

use crate::state::ArkaWm;

#[derive(Default)]
pub struct Ipc {
    watchers: Vec<UnixStream>,
    /// The last window list sent, so it is only sent again when it changes.
    last: String,
}

/// One window as the shell sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: u32,
    pub focused: bool,
    pub minimized: bool,
    pub app_id: String,
    pub title: String,
}

/// Tabs, line breaks and separators cannot appear inside a field.
fn clean(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

pub fn encode(entries: &[Entry]) -> String {
    let records: Vec<String> = entries
        .iter()
        .map(|e| format!("{}\t{}\t{}\t{}\t{}", e.id, e.focused as u8, e.minimized as u8, clean(&e.app_id), clean(&e.title)))
        .collect();
    format!("windows {}\n", records.join("\u{1f}"))
}

/// A command from the shell.
#[derive(Debug, PartialEq)]
pub enum Command {
    Watch,
    /// Go to this window; if it is already in front, minimise it (a taskbar click).
    Toggle(u32),
    Focus(u32),
    Minimize(u32),
    Close(u32),
    ShowDesktop,
    Screenshot,
    Region,
    /// Night light on or off.
    Night(bool),
}

pub fn parse(line: &str) -> Option<Command> {
    let mut words = line.split_whitespace();
    let id = |words: &mut std::str::SplitWhitespace| words.next()?.parse().ok();
    Some(match words.next()? {
        "watch" => Command::Watch,
        "toggle" => Command::Toggle(id(&mut words)?),
        "focus" => Command::Focus(id(&mut words)?),
        "minimize" => Command::Minimize(id(&mut words)?),
        "close" => Command::Close(id(&mut words)?),
        "show-desktop" => Command::ShowDesktop,
        "screenshot" => Command::Screenshot,
        "region" => Command::Region,
        "night" => match words.next()? {
            "on" => Command::Night(true),
            "off" => Command::Night(false),
            _ => return None,
        },
        _ => return None,
    })
}

/// Open the socket and tell programs started from ArkaWM where it is (`ARKAWM_SOCKET`).
pub fn start(handle: &LoopHandle<'static, ArkaWm>, display: &std::ffi::OsStr) {
    let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") else { return };
    let path = std::path::Path::new(&dir).join(format!("arkawm-{}.sock", display.to_string_lossy()));
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::warn!("no shell socket: {err}");
            return;
        }
    };
    let _ = listener.set_nonblocking(true);
    std::env::set_var("ARKAWM_SOCKET", &path);
    let clients = handle.clone();
    let _ = handle.insert_source(Generic::new(listener, Interest::READ, Mode::Level), move |_, listener, _| {
        while let Ok((stream, _)) = listener.accept() {
            let _ = stream.set_nonblocking(true);
            let mut pending = String::new();
            let _ = clients.insert_source(Generic::new(stream, Interest::READ, Mode::Level), move |_, stream, state| {
                let mut bytes = [0u8; 1024];
                let mut reader: &UnixStream = &**stream;
                match reader.read(&mut bytes) {
                    Ok(0) => return Ok(PostAction::Remove),
                    Ok(n) => pending.push_str(&String::from_utf8_lossy(&bytes[..n])),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => return Ok(PostAction::Continue),
                    Err(_) => return Ok(PostAction::Remove),
                }
                while let Some(end) = pending.find('\n') {
                    let line: String = pending.drain(..=end).collect();
                    match parse(&line) {
                        Some(Command::Watch) => {
                            if let Ok(copy) = stream.try_clone() {
                                state.ipc.watchers.push(copy);
                                // send the list to the newcomer on the next round
                                state.ipc.last.clear();
                            }
                        }
                        Some(command) => state.shell_command(command),
                        None => tracing::warn!("shell socket: not understood: {}", line.trim()),
                    }
                }
                // A flood of unterminated bytes is not a command.
                if pending.len() > 4096 {
                    return Ok(PostAction::Remove);
                }
                Ok(PostAction::Continue)
            });
        }
        Ok(PostAction::Continue)
    });
}

impl ArkaWm {
    fn window_with_id(&self, id: u32) -> Option<usize> {
        self.windows.iter().position(|m| m.id == id)
    }

    fn shell_command(&mut self, command: Command) {
        self.redraw_wanted = true;
        match command {
            Command::Watch => {}
            Command::Toggle(id) => {
                if let Some(index) = self.window_with_id(id) {
                    if index == self.focused && !self.windows[index].minimized {
                        self.set_minimized(index, true);
                    } else {
                        self.focus_index(index);
                    }
                }
            }
            Command::Focus(id) => {
                if let Some(index) = self.window_with_id(id) {
                    self.focus_index(index);
                }
            }
            Command::Minimize(id) => {
                if let Some(index) = self.window_with_id(id) {
                    self.set_minimized(index, true);
                }
            }
            Command::Close(id) => {
                if let Some(toplevel) = self.window_with_id(id).and_then(|i| self.windows[i].window.toplevel()) {
                    toplevel.send_close();
                }
            }
            Command::ShowDesktop => self.toggle_show_desktop(),
            Command::Screenshot => self.capture = Some(crate::capture::Capture { crop: None }),
            Command::Region => self.region = Some(crate::capture::Region::default()),
            Command::Night(on) => {
                self.night = on;
                match self.udev.as_ref() {
                    Some(udev) => udev.set_warmth(on),
                    None => tracing::info!("night light needs ArkaWM on the real screen"),
                }
            }
        }
    }

    pub fn window_entries(&self) -> Vec<Entry> {
        self.windows
            .iter()
            .enumerate()
            .map(|(index, managed)| {
                let app_id = managed
                    .window
                    .toplevel()
                    .map(|toplevel| {
                        smithay::wayland::compositor::with_states(toplevel.wl_surface(), |states| {
                            states
                                .data_map
                                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                .and_then(|data| data.lock().unwrap().app_id.clone())
                                .unwrap_or_default()
                        })
                    })
                    .unwrap_or_default();
                Entry {
                    id: managed.id,
                    focused: index == self.focused && !managed.minimized,
                    minimized: managed.minimized,
                    app_id,
                    title: Self::window_title(&managed.window),
                }
            })
            .collect()
    }

    /// Send a line to every watching shell; one that has gone away is dropped.
    pub fn tell_shell(&mut self, line: &str) {
        self.ipc.watchers.retain_mut(|stream| stream.write_all(line.as_bytes()).is_ok());
    }

    /// After each round of events: send the window list if it changed.
    pub fn publish_windows(&mut self) {
        if self.ipc.watchers.is_empty() {
            return;
        }
        let line = encode(&self.window_entries());
        if line != self.ipc.last {
            self.tell_shell(&line);
            self.ipc.last = line;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_read() {
        assert_eq!(parse("watch\n"), Some(Command::Watch));
        assert_eq!(parse("toggle 7"), Some(Command::Toggle(7)));
        assert_eq!(parse("close 12\n"), Some(Command::Close(12)));
        assert_eq!(parse("show-desktop"), Some(Command::ShowDesktop));
        assert_eq!(parse("focus"), None);
        assert_eq!(parse("focus x"), None);
        assert_eq!(parse("rm -rf"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn a_title_cannot_break_the_line() {
        let entry = Entry { id: 3, focused: true, minimized: false, app_id: "org.kde.kate".into(), title: "a\tb\nc\u{1f}d".into() };
        assert_eq!(encode(&[entry]), "windows 3\t1\t0\torg.kde.kate\ta b c d\n");
        assert_eq!(encode(&[]), "windows \n");
    }
}
