use smithay::{
    delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, PopupKeyboardGrab, PopupKind, PopupManager,
        PopupPointerGrab, PopupUngrabStrategy, Space, Window,
    },
    input::{
        pointer::{Focus, GrabStartData as PointerGrabStartData},
        Seat,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            protocol::{wl_output, wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::Serial,
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    grabs::{MoveGrab, ResizeGrab},
    state::WindowState,
    ArkaWm,
};

impl XdgShellHandler for ArkaWm {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.add_window(Window::new_wayland_window(surface));
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.remove_window(surface.wl_surface());
    }

    /// The app's title bar was dragged.
    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some((start_data, window)) = self.grab_target(&surface, &seat, serial) else { return };
        if let Some(grab) = MoveGrab::start(self, start_data, window) {
            self.seat.get_pointer().unwrap().set_grab(self, grab, serial, Focus::Clear);
        }
    }

    /// An edge of the app's window was dragged.
    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let Some((start_data, window)) = self.grab_target(&surface, &seat, serial) else { return };
        if let Some(grab) = ResizeGrab::start(self, start_data, window, edges.into()) {
            self.seat.get_pointer().unwrap().set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self.index_of(surface.wl_surface()) {
            self.set_minimized(index, true);
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self.index_of(surface.wl_surface()) {
            self.set_state(index, WindowState::Maximized);
        }
        // The protocol wants a configure in reply even if nothing changed.
        if surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self.index_of(surface.wl_surface()) {
            self.set_state(index, WindowState::Normal);
        }
        if surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<wl_output::WlOutput>) {
        if let Some(index) = self.index_of(surface.wl_surface()) {
            self.set_fullscreen(index, true);
        }
        if surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self.index_of(surface.wl_surface()) {
            self.set_fullscreen(index, false);
        }
        if surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    /// A menu takes the keyboard and pointer while it is open, so a click anywhere outside the
    /// app closes it and the arrow keys and Escape go to the menu.
    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        // No window gets the keyboard or pointer while the screen is locked.
        if self.locked {
            return;
        }
        let Some(seat) = Seat::<ArkaWm>::from_resource(&seat) else { return };
        let kind = PopupKind::Xdg(surface);
        // The window or panel the menu belongs to.
        let Ok(root) = find_popup_root_surface(&kind) else { return };
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else { return };

        if let Some(keyboard) = seat.get_keyboard() {
            // Something else (a window being dragged, say) already holds the input: refuse the menu's grab.
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial) || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }
}

// Xdg Shell
delegate_xdg_shell!(ArkaWm);

/// Should be called on `WlSurface::commit`
pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    // Handle toplevel commits.
    if let Some(window) = space
        .elements()
        .find(|w| w.toplevel().unwrap().wl_surface() == surface)
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.toplevel().unwrap().send_configure();
        }
    }

    // Handle popup commits.
    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    // NOTE: This should never fail as the initial configure is always
                    // allowed.
                    xdg.send_configure().expect("initial configure failed");
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl ArkaWm {
    /// The pointer grab data for a move or resize the client asked for, if
    /// the click it refers to is still held on that client's surface.
    fn grab_target(
        &self,
        surface: &ToplevelSurface,
        seat: &wl_seat::WlSeat,
        serial: Serial,
    ) -> Option<(PointerGrabStartData<ArkaWm>, Window)> {
        let seat = Seat::<ArkaWm>::from_resource(seat)?;
        let pointer = seat.get_pointer()?;
        if !pointer.has_grab(serial) {
            return None;
        }
        let start_data = pointer.grab_start_data()?;
        let (focus, _) = start_data.focus.as_ref()?;
        if !focus.id().same_client_as(&surface.wl_surface().id()) {
            return None;
        }
        let window = self
            .windows
            .iter()
            .find(|m| m.window.toplevel().map(|t| t == surface).unwrap_or(false))?
            .window
            .clone();
        Some((start_data, window))
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == &root)
        else {
            return;
        };

        let output = self.space.outputs().next().unwrap();
        let output_geo = self.space.output_geometry(output).unwrap();
        let window_geo = self.space.element_geometry(window).unwrap();

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

//
// Title bars: the app says whether it wants ArkaWM to draw one
//

use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;

impl ArkaWm {
    fn set_decorated(&mut self, toplevel: &ToplevelSurface, on: bool) {
        let mode = if on { DecorationMode::ServerSide } else { DecorationMode::ClientSide };
        toplevel.with_pending_state(|state| state.decoration_mode = Some(mode));
        if let Some(index) = self.index_of(toplevel.wl_surface()) {
            self.windows[index].decorated = on;
            self.arrange();
        }
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

impl XdgDecorationHandler for ArkaWm {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.set_decorated(&toplevel, true);
    }

    /// An app that prefers its own title bar keeps it.
    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        self.set_decorated(&toplevel, mode != DecorationMode::ClientSide);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.set_decorated(&toplevel, true);
    }
}
smithay::delegate_xdg_decoration!(ArkaWm);
