//! The list of open windows for a dock: wlr-foreign-toplevel-management. Crystal Dock (the
//! Classic theme's dock) reads each window's title, app and state from it, and asks through it to
//! bring a window forward, minimise, maximise or close it. Crystal Dock does not start without it.
//!
//! Seeing every window's title is a lot to hand any app, so only programs on the trust list for
//! windows get this protocol (see trust.rs). When no dock is connected this costs nothing: the
//! list is only worked out while someone is listening.

use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};
use smithay::reexports::wayland_server::{
    backend::{ClientId, GlobalId},
    protocol::wl_output::WlOutput,
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::state::{ArkaWm, WindowState};

const VERSION: u32 = 3;

#[derive(Default)]
pub struct Toplevels {
    managers: Vec<ZwlrForeignToplevelManagerV1>,
    sent: Vec<Sent>,
}

/// One window as one dock last heard about it.
struct Sent {
    window: u32,
    handle: ZwlrForeignToplevelHandleV1,
    title: Option<String>,
    app_id: Option<String>,
    states: Option<Vec<u32>>,
    outputs: Vec<WlOutput>,
}

/// What a dock is told about one window.
#[derive(Debug, PartialEq)]
pub struct Shown {
    pub id: u32,
    pub title: String,
    pub app_id: String,
    pub states: Vec<u32>,
}

/// The protocol's state values for a window.
pub fn states(focused: bool, minimized: bool, state: WindowState) -> Vec<u32> {
    use zwlr_foreign_toplevel_handle_v1::State;
    let mut out = Vec::new();
    if state == WindowState::Maximized {
        out.push(State::Maximized as u32);
    }
    if minimized {
        out.push(State::Minimized as u32);
    }
    if focused && !minimized {
        out.push(State::Activated as u32);
    }
    if state == WindowState::Fullscreen {
        out.push(State::Fullscreen as u32);
    }
    out
}

/// The state values as the bytes of the protocol's array: native-endian u32s.
fn state_bytes(states: &[u32]) -> Vec<u8> {
    states.iter().flat_map(|s| s.to_ne_bytes()).collect()
}

pub fn create_global(dh: &DisplayHandle) -> GlobalId {
    dh.create_global::<ArkaWm, ZwlrForeignToplevelManagerV1, _>(VERSION, ())
}

impl ArkaWm {
    /// After each round of events: tell every dock what changed in the window list.
    pub fn sync_toplevels(&mut self) {
        self.toplevels.managers.retain(|m| m.is_alive());
        self.toplevels.sent.retain(|s| s.handle.is_alive());
        if self.toplevels.managers.is_empty() {
            return;
        }
        let shown: Vec<(Shown, Vec<Output>)> = self
            .window_entries()
            .into_iter()
            .zip(self.windows.iter())
            .map(|(entry, managed)| {
                let mut outputs = self.space.outputs_for_element(&managed.window);
                // a minimised window is off the screen; it still belongs to the screen(s)
                if outputs.is_empty() {
                    outputs = self.space.outputs().cloned().collect();
                }
                let shown = Shown {
                    id: entry.id,
                    title: entry.title,
                    app_id: entry.app_id,
                    states: states(entry.focused, entry.minimized, managed.state),
                };
                (shown, outputs)
            })
            .collect();

        // windows that have closed
        self.toplevels.sent.retain(|sent| {
            let open = shown.iter().any(|(s, _)| s.id == sent.window);
            if !open {
                sent.handle.closed();
            }
            open
        });

        let dh = self.display_handle.clone();
        for manager in self.toplevels.managers.clone() {
            let Some(client) = manager.client() else { continue };
            for (window, outputs) in &shown {
                let at = self.toplevels.sent.iter().position(|s| s.window == window.id && same_client(&s.handle, &client.id()));
                let at = match at {
                    Some(at) => at,
                    None => {
                        let Ok(handle) = client.create_resource::<ZwlrForeignToplevelHandleV1, u32, ArkaWm>(&dh, manager.version(), window.id)
                        else {
                            continue;
                        };
                        manager.toplevel(&handle);
                        self.toplevels.sent.push(Sent { window: window.id, handle, title: None, app_id: None, states: None, outputs: Vec::new() });
                        self.toplevels.sent.len() - 1
                    }
                };
                let sent = &mut self.toplevels.sent[at];
                let mut changed = false;
                if sent.title.as_deref() != Some(&window.title) {
                    sent.handle.title(window.title.clone());
                    sent.title = Some(window.title.clone());
                    changed = true;
                }
                if sent.app_id.as_deref() != Some(&window.app_id) {
                    sent.handle.app_id(window.app_id.clone());
                    sent.app_id = Some(window.app_id.clone());
                    changed = true;
                }
                let wanted: Vec<WlOutput> = outputs.iter().flat_map(|o| o.client_outputs(&client)).collect();
                for gone in sent.outputs.iter().filter(|o| !wanted.contains(o)) {
                    if gone.is_alive() {
                        sent.handle.output_leave(gone);
                    }
                    changed = true;
                }
                for new in wanted.iter().filter(|o| !sent.outputs.contains(o)) {
                    sent.handle.output_enter(new);
                    changed = true;
                }
                sent.outputs = wanted;
                if sent.states.as_ref() != Some(&window.states) {
                    sent.handle.state(state_bytes(&window.states));
                    sent.states = Some(window.states.clone());
                    changed = true;
                }
                if changed {
                    sent.handle.done();
                }
            }
        }
    }

    fn window_index_by_id(&self, id: u32) -> Option<usize> {
        self.windows.iter().position(|m| m.id == id)
    }
}

fn same_client(handle: &ZwlrForeignToplevelHandleV1, client: &ClientId) -> bool {
    handle.client().is_some_and(|c| &c.id() == client)
}

impl GlobalDispatch<ZwlrForeignToplevelManagerV1, ()> for ArkaWm {
    fn bind(
        state: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, ());
        state.toplevels.managers.push(manager);
        // the windows are sent after this round of events (sync_toplevels)
    }

    fn can_view(client: Client, _data: &()) -> bool {
        crate::trust::client_trusted(&client, crate::trust::Role::Windows)
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for ArkaWm {
    fn request(
        state: &mut Self,
        _client: &Client,
        manager: &ZwlrForeignToplevelManagerV1,
        request: zwlr_foreign_toplevel_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = request {
            state.toplevels.managers.retain(|m| m != manager);
            manager.finished();
        }
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, u32> for ArkaWm {
    fn request(
        state: &mut Self,
        _client: &Client,
        _handle: &ZwlrForeignToplevelHandleV1,
        request: zwlr_foreign_toplevel_handle_v1::Request,
        id: &u32,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_foreign_toplevel_handle_v1::Request;
        let Some(index) = state.window_index_by_id(*id) else { return };
        state.redraw_wanted = true;
        match request {
            Request::Activate { .. } | Request::UnsetMinimized => state.focus_index(index),
            Request::SetMinimized => state.set_minimized(index, true),
            Request::SetMaximized => state.set_state(index, WindowState::Maximized),
            Request::UnsetMaximized => {
                if state.windows[index].state == WindowState::Maximized {
                    state.set_state(index, WindowState::Normal);
                }
            }
            Request::SetFullscreen { .. } => state.set_fullscreen(index, true),
            Request::UnsetFullscreen => state.set_fullscreen(index, false),
            Request::Close => {
                if let Some(toplevel) = state.windows[index].window.toplevel() {
                    toplevel.send_close();
                }
            }
            // where the dock's button is, for a minimise animation: ArkaWM has none
            Request::SetRectangle { .. } | Request::Destroy => {}
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_states_follow_the_protocol() {
        // maximized 0, minimized 1, activated 2, fullscreen 3
        assert_eq!(states(true, false, WindowState::Maximized), [0, 2]);
        assert_eq!(states(true, true, WindowState::Normal), [1]);
        assert_eq!(states(false, false, WindowState::Fullscreen), [3]);
        assert_eq!(state_bytes(&[2]), 2u32.to_ne_bytes());
    }
}
