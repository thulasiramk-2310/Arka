use std::{ffi::OsString, sync::Arc};

use smithay::{
    desktop::{layer_map_for_output, PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    backend::{
        renderer::{element::solid::SolidColorBuffer, gles::GlesRenderer},
        winit::WinitGraphicsBackend,
    },
    utils::{Logical, Point, Rectangle},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        dmabuf::{DmabufGlobal, DmabufState},
        output::OutputManagerState,
        selection::{data_device::DataDeviceState, primary_selection::PrimarySelectionState},
        shell::{wlr_layer::WlrLayerShellState, xdg::XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
        viewporter::ViewporterState,
        xdg_activation::XdgActivationState,
        fractional_scale::FractionalScaleManagerState,
        idle_inhibit::IdleInhibitManagerState,
        idle_notify::IdleNotifierState,
        session_lock::{LockSurface, SessionLockManagerState, SessionLocker},
    },
};

use smithay::wayland::shell::wlr_layer::Layer;


pub struct ArkaWm {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    pub loop_handle: smithay::reexports::calloop::LoopHandle<'static, ArkaWm>,
    /// The timer repeating a held volume or brightness key.
    pub media_repeat: Option<smithay::reexports::calloop::RegistrationToken>,

    /// Every window, in the order they were opened (and their row order).
    pub windows: Vec<Managed>,
    /// Index into `windows` of the focused window.
    pub focused: usize,
    /// Normal overlapping windows, or the scrolling row.
    pub mode: LayoutMode,
    /// How far the row is scrolled, in logical pixels.
    pub view_x: i32,
    /// Where a dragged window will snap if released now.
    /// The link to the desktop shell.
    pub ipc: crate::ipc::Ipc,
    /// A screenshot to take with the next frame.
    pub capture: Option<crate::capture::Capture>,
    /// A part of the screen is being picked for a screenshot.
    pub region: Option<crate::capture::Region>,
    pub region_buffers: [SolidColorBuffer; 4],
    /// The number the next window gets; the shell names windows by it.
    pub next_window_id: u32,
    /// The windows "show desktop" put away, bottom to top, to bring back the same way.
    pub desktop_hidden: Vec<u32>,
    pub data_control_state: smithay::wayland::selection::wlr_data_control::DataControlState,
    /// The Alt+Tab window switcher, while it is open.
    pub switcher: Option<crate::switcher::Switcher>,
    pub snap_preview: Option<Rectangle<i32, Logical>>,
    pub snap_preview_buffer: SolidColorBuffer,

    /// The nested window ArkaWM draws into (a real session will use the GPU directly).
    pub backend: Option<WinitGraphicsBackend<GlesRenderer>>,
    pub dmabuf: Option<(DmabufState, DmabufGlobal)>,
    /// Set instead of `backend` when ArkaWM runs directly on the hardware.
    pub udev: Option<Box<crate::udev::Udev>>,
    /// Something may have changed on screen since the last frame (an app spoke, or the user did
    /// something). On hardware a frame is drawn only when this is set, so an idle desktop costs nothing.
    pub redraw_wanted: bool,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<ArkaWm>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub viewporter_state: ViewporterState,
    pub primary_selection_state: PrimarySelectionState,
    pub activation_state: XdgActivationState,
    pub session_lock_state: SessionLockManagerState,
    pub idle_notifier_state: IdleNotifierState<Self>,
    pub idle_inhibit_state: IdleInhibitManagerState,

    /// How much everything is enlarged: 1.0, 1.25, 1.5 and so on.
    pub scale: f64,
    pub fractional_scale_state: FractionalScaleManagerState,

    /// The screen is locked: nothing but the lock screen is drawn or gets input.
    pub locked: bool,
    /// The lock screen's own surface, once the locker has made one.
    pub lock_surface: Option<LockSurface>,
    /// The locker is told "locked" only after a frame without any window has been drawn.
    pub lock_pending: Option<SessionLocker>,
    /// Surfaces asking that the screen not go idle (a playing video).
    pub idle_inhibitors: Vec<WlSurface>,
    pub decoration_state: smithay::wayland::shell::xdg::decoration::XdgDecorationState,
    /// The last click on a title bar, to recognise a double click.
    pub last_bar_click: Option<(std::time::Instant, usize)>,
    /// The title-bar button under the pointer, lit in its colour.
    pub bar_hover: Option<(usize, crate::titlebar::Button)>,
    /// Developer test hook: save the next frame to this file (PPM).
    pub shot_request: Option<std::path::PathBuf>,
    /// The picture of ArkaWM's own pointer, drawn when running on hardware.
    pub pointer_buffer: Option<smithay::backend::renderer::element::memory::MemoryRenderBuffer>,
    /// What the app under the pointer wants the pointer to look like.
    pub cursor_status: smithay::input::pointer::CursorImageStatus,
    /// Pointer shapes already loaded from the cursor theme (`None`: the theme lacks that shape).
    pub cursor_cache: std::collections::HashMap<smithay::input::pointer::CursorIcon, Option<crate::cursor::Picture>>,
    pub cursor_shape_state: smithay::wayland::cursor_shape::CursorShapeManagerState,

    pub seat: Seat<Self>,
}

impl ArkaWm {
    pub fn new(event_loop: &mut EventLoop<'static, Self>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let popups = PopupManager::default();
        let viewporter_state = ViewporterState::new::<Self>(&dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        let activation_state = XdgActivationState::new::<Self>(&dh);
        let session_lock_state = SessionLockManagerState::new::<Self, _>(&dh, |client| {
            crate::trust::client_trusted(client, crate::trust::Role::Locker)
        });
        let idle_notifier_state = IdleNotifierState::new(&dh, event_loop.handle());
        let idle_inhibit_state = IdleInhibitManagerState::new::<Self>(&dh);
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(&dh);
        // Typing in other scripts (Tamil, Hindi ...): the input-method program composes the text,
        // text-input hands it to the app, virtual-keyboard lets it send plain keys.
        smithay::wayland::text_input::TextInputManagerState::new::<Self>(&dh);
        smithay::wayland::input_method::InputMethodManagerState::new::<Self, _>(&dh, |client| {
            crate::trust::client_trusted(client, crate::trust::Role::Typing)
        });
        smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState::new::<Self, _>(&dh, |client| {
            crate::trust::client_trusted(client, crate::trust::Role::Typing)
        });

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 200, 25).unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        let socket_name = Self::init_wayland_listener(display, event_loop);

        // Get the loop signal, used to stop the event loop
        let loop_signal = event_loop.get_signal();

        Self {
            start_time,
            display_handle: dh.clone(),

            space,
            loop_signal,
            loop_handle: event_loop.handle(),
            media_repeat: None,
            windows: Vec::new(),
            focused: 0,
            mode: LayoutMode::Free,
            view_x: 0,
            ipc: Default::default(),
            capture: None,
            region: None,
            region_buffers: Default::default(),
            next_window_id: 1,
            desktop_hidden: Vec::new(),
            data_control_state: smithay::wayland::selection::wlr_data_control::DataControlState::new::<Self, _>(
                &dh,
                Some(&primary_selection_state),
                |client| crate::trust::client_trusted(client, crate::trust::Role::Clipboard),
            ),
            switcher: None,
            snap_preview: None,
            snap_preview_buffer: SolidColorBuffer::default(),
            backend: None,
            dmabuf: None,
            udev: None,
            redraw_wanted: true,
            socket_name,

            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            viewporter_state,
            primary_selection_state,
            activation_state,
            session_lock_state,
            idle_notifier_state,
            idle_inhibit_state,
            scale: scale_from(std::env::var("ARKAWM_SCALE").ok().as_deref()),
            fractional_scale_state,
            locked: false,
            lock_surface: None,
            lock_pending: None,
            idle_inhibitors: Vec::new(),
            decoration_state: smithay::wayland::shell::xdg::decoration::XdgDecorationState::new::<Self>(&dh),
            last_bar_click: None,
            bar_hover: None,
            shot_request: None,
            pointer_buffer: None,
            cursor_status: smithay::input::pointer::CursorImageStatus::default_named(),
            cursor_cache: Default::default(),
            cursor_shape_state: smithay::wayland::cursor_shape::CursorShapeManagerState::new::<Self>(&dh),
            seat,
        }
    }

    fn init_wayland_listener(
        display: Display<ArkaWm>,
        event_loop: &mut EventLoop<'static, Self>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                let data = Arc::new(ClientState::default());
                let Ok(client) = state.display_handle.insert_client(client_stream, data.clone()) else { return };
                // Record which program this is, for the protocols only trusted programs may use.
                if let Ok(credentials) = client.get_credentials(&state.display_handle) {
                    if let Some(program) = crate::trust::program_of(credentials.pid) {
                        let _ = data.program.set(program);
                    }
                }
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // Safety: we don't drop the display
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }
                    state.redraw_wanted = true;
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    /// The surface under the pointer: panels and docks above windows first,
    /// then windows, then background layers.
    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        let output = self.space.outputs().next()?;
        let output_geo = self.space.output_geometry(output)?;
        let layers = layer_map_for_output(output);
        let local = pos - output_geo.loc.to_f64();

        // Locked: the lock screen is the only thing that can be pointed at.
        if self.locked {
            let surface = self.lock_surface.as_ref()?.wl_surface();
            return smithay::desktop::utils::under_from_surface_tree(surface, local, (0, 0), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + output_geo.loc).to_f64()));
        }

        let layer_hit = |kinds: [Layer; 2]| {
            kinds.iter().find_map(|&kind| {
                let layer = layers.layer_under(kind, local)?;
                let layer_loc = layers.layer_geometry(layer)?.loc;
                layer
                    .surface_under(local - layer_loc.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, (p + layer_loc + output_geo.loc).to_f64()))
            })
        };

        // A full-screen window with the focus is above the panels, so it gets the pointer first.
        let fullscreen = self.fullscreen_window().and_then(|window| {
            let location = self.space.element_location(&window)?;
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + location).to_f64()))
        });
        if fullscreen.is_some() {
            return fullscreen;
        }

        layer_hit([Layer::Overlay, Layer::Top])
            .or_else(|| {
                // A title bar belongs to ArkaWM, not to the app under it.
                if self.bar_at(pos).is_some() {
                    return None;
                }
                self.space.element_under(pos).and_then(|(window, location)| {
                    window
                        .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                        .map(|(s, p)| (s, (p + location).to_f64()))
                })
            })
            .or_else(|| layer_hit([Layer::Bottom, Layer::Background]))
    }
}

/// A window and how ArkaWM places it.
pub struct Managed {
    pub window: Window,
    /// A number that stays the same for the life of the window.
    pub id: u32,
    /// Put away: not on screen, still in the window list.
    pub minimized: bool,
    /// Position and size when neither maximised nor snapped.
    pub rect: Rectangle<i32, Logical>,
    pub state: WindowState,
    /// What to go back to when full screen ends.
    pub restore: WindowState,
    /// Centred once its first frame arrives.
    pub placed: bool,
    /// Share of the screen width in the scrolling row.
    pub row_width: f64,
    /// ArkaWM draws this window's title bar (the app asked for one).
    pub decorated: bool,
    /// The drawn bar and what it was drawn from.
    pub bar: Option<(crate::titlebar::BarKey, smithay::backend::renderer::element::memory::MemoryRenderBuffer)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    Normal,
    Maximized,
    Snapped(Snap),
    /// Covers the whole screen, panels and dock included, while it has the focus.
    Fullscreen,
}

/// Screen halves and quarters a window can snap to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Snap {
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    /// Overlapping windows that snap to halves, quarters or full screen.
    Free,
    /// Windows side by side at full height, scrolling sideways.
    Row,
}

/// The scale asked for with `ARKAWM_SCALE`, kept between 1 and 3 and on the 1/120 steps the
/// fractional-scale protocol can express. Anything unreadable means 1.
pub fn scale_from(value: Option<&str>) -> f64 {
    let scale = value.and_then(|v| v.trim().parse::<f64>().ok()).filter(|s| s.is_finite()).unwrap_or(1.0);
    (scale.clamp(1.0, 3.0) * 120.0).round() / 120.0
}

/// Put the pointer in the middle of the screen (on hardware it would otherwise start in a corner).
pub fn center_pointer(state: &mut ArkaWm) {
    let area = state.output_area();
    let middle = (area.loc.x as f64 + area.size.w as f64 / 2.0, area.loc.y as f64 + area.size.h as f64 / 2.0);
    state.pointer_move(middle.into(), 0);
}

/// A sensible scale for a screen from its pixel width and physical width, when none was asked for:
/// about 120 dots per inch counts as 1, in quarter steps, between 1 and 3. Unknown size means 1.
pub fn scale_for_screen(width_px: i32, width_mm: i32) -> f64 {
    if width_px <= 0 || width_mm <= 0 {
        return 1.0;
    }
    let dpi = width_px as f64 / (width_mm as f64 / 25.4);
    ((dpi / 120.0) * 4.0).round().clamp(4.0, 12.0) / 4.0
}

/// Start a program inside ArkaWM.
pub fn spawn(command: &str) {
    if let Err(err) = std::process::Command::new("sh").arg("-c").arg(command).spawn() {
        tracing::warn!("could not start {command}: {err}");
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    /// The program file behind this connection, recorded when it connected.
    pub program: std::sync::OnceLock<std::path::PathBuf>,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, reason: DisconnectReason) {
        tracing::debug!("a program disconnected: {reason:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::scale_from;

    #[test]
    fn scale_is_read_clamped_and_stepped() {
        // a 14 inch 2880 px laptop panel, a 14 inch full-HD one, a 24 inch full-HD monitor, unknown
        assert_eq!(super::scale_for_screen(2880, 302), 2.0);
        assert_eq!(super::scale_for_screen(1920, 309), 1.25);
        assert_eq!(super::scale_for_screen(1920, 527), 1.0);
        assert_eq!(super::scale_for_screen(1920, 0), 1.0);
        assert_eq!(scale_from(None), 1.0);
        assert_eq!(scale_from(Some("1.5")), 1.5);
        assert_eq!(scale_from(Some(" 1.25 ")), 1.25);
        assert_eq!(scale_from(Some("0.2")), 1.0);
        assert_eq!(scale_from(Some("9")), 3.0);
        assert_eq!(scale_from(Some("big")), 1.0);
        assert_eq!(scale_from(Some("NaN")), 1.0);
        // 1.333 is not a 1/120 step; the nearest one is 160/120
        assert_eq!(scale_from(Some("1.333")), 160.0 / 120.0);
    }
}
