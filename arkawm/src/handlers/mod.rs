mod compositor;
mod layer_shell;
mod session_lock;
mod xdg_shell;

use crate::ArkaWm;

//
// Wl Seat
//

use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::{delegate_data_device, delegate_output, delegate_seat};

impl SeatHandler for ArkaWm {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<ArkaWm> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: smithay::input::pointer::CursorImageStatus) {
        self.cursor_status = image;
        self.redraw_wanted = true;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client.clone());
        smithay::wayland::selection::primary_selection::set_primary_focus(dh, seat, client);
    }
}

delegate_seat!(ArkaWm);

// Apps name the pointer shape they want (text beam, hand, resize arrows) instead of sending a picture.
impl smithay::wayland::tablet_manager::TabletSeatHandler for ArkaWm {}
smithay::delegate_cursor_shape!(ArkaWm);

//
// Wl Data Device
//

impl SelectionHandler for ArkaWm {
    type SelectionUserData = ();

    /// Remember which program copied: a program can only copy while it has the keyboard.
    fn new_selection(
        &mut self,
        ty: smithay::wayland::selection::SelectionTarget,
        source: Option<smithay::wayland::selection::SelectionSource>,
        _seat: smithay::input::Seat<Self>,
    ) {
        if ty == smithay::wayland::selection::SelectionTarget::Clipboard {
            use smithay::reexports::wayland_server::Resource;
            self.clip_owner = source
                .and_then(|_| self.seat.get_keyboard()?.current_focus())
                .and_then(|surface| surface.client())
                .map(|client| client.id());
        }
    }
}

impl smithay::wayland::selection::wlr_data_control::DataControlHandler for ArkaWm {
    fn data_control_state(&self) -> &smithay::wayland::selection::wlr_data_control::DataControlState {
        &self.data_control_state
    }
}
smithay::delegate_data_control!(ArkaWm);

impl DataDeviceHandler for ArkaWm {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for ArkaWm {}
impl ServerDndGrabHandler for ArkaWm {}

delegate_data_device!(ArkaWm);

//
// Wl Output & Xdg Output
//

impl OutputHandler for ArkaWm {}
delegate_output!(ArkaWm);

//
// linux-dmabuf: GPU buffers from apps
//

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::delegate_dmabuf;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};

impl DmabufHandler for ArkaWm {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf.as_mut().expect("dmabuf global exists only once set up").0
    }

    /// Accept a GPU buffer only if the renderer can actually use it.
    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        let imported = match (self.backend.as_mut(), self.udev.as_mut()) {
            (Some(backend), _) => backend.renderer().import_dmabuf(&dmabuf, None).is_ok(),
            (None, Some(udev)) => udev.renderer.import_dmabuf(&dmabuf, None).is_ok(),
            (None, None) => false,
        };
        if imported {
            let _ = notifier.successful::<ArkaWm>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(ArkaWm);

//
// Viewporter: lets an app crop and scale its buffer (xwayland-satellite needs it)
//

smithay::delegate_viewporter!(ArkaWm);

//
// Primary selection: select text, paste it with the middle button
//

use smithay::wayland::selection::primary_selection::{PrimarySelectionHandler, PrimarySelectionState};

impl PrimarySelectionHandler for ArkaWm {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}
smithay::delegate_primary_selection!(ArkaWm);

//
// xdg-activation: an app asks for another window to get the focus (a link opening the browser)
//

use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};

/// A request is honoured only this long after the user's action that produced it.
const ACTIVATION_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

/// Whether a focus request made `age` after its token was issued is still honoured.
pub fn activation_fresh(age: std::time::Duration) -> bool {
    age < ACTIVATION_WINDOW
}

impl XdgActivationHandler for ArkaWm {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.activation_state
    }

    /// A token counts only if the app asking held the keyboard focus when it asked: it must carry
    /// an input serial from our seat that is no older than the last time the focus moved.
    /// Anything else (a background app trying to pull the focus to itself) gets no token.
    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        let Some((serial, seat)) = data.serial else { return false };
        let Some(seat) = Seat::<ArkaWm>::from_resource(&seat) else { return false };
        seat.get_keyboard()
            .and_then(|keyboard| keyboard.last_enter())
            .map(|last_enter| serial.is_no_older_than(&last_enter))
            .unwrap_or(false)
    }

    fn request_activation(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData, surface: WlSurface) {
        if !activation_fresh(data.timestamp.elapsed()) {
            return;
        }
        if let Some(index) = self.index_of(&surface) {
            self.focus_index(index);
        }
    }
}
smithay::delegate_xdg_activation!(ArkaWm);

#[cfg(test)]
mod tests {
    use super::activation_fresh;
    use std::time::Duration;

    #[test]
    fn focus_requests_expire() {
        assert!(activation_fresh(Duration::from_secs(1)));
        assert!(!activation_fresh(Duration::from_secs(10)));
        assert!(!activation_fresh(Duration::from_secs(60)));
    }
}

//
// Fractional scale: tells each app the exact scale (1.25, 1.5 ...) so it draws sharp, not stretched
//

use smithay::wayland::fractional_scale::{with_fractional_scale, FractionalScaleHandler};

impl FractionalScaleHandler for ArkaWm {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.scale;
        smithay::wayland::compositor::with_states(&surface, |states| {
            with_fractional_scale(states, |fractional| fractional.set_preferred_scale(scale));
        });
    }
}
smithay::delegate_fractional_scale!(ArkaWm);

//
// Input methods: typing in scripts a keyboard cannot type directly (Tamil, Hindi, Malayalam ...)
//

use smithay::desktop::{PopupKind, PopupManager};
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::input_method::{InputMethodHandler, PopupSurface as ImePopupSurface};

impl InputMethodHandler for ArkaWm {
    /// The small box of suggestions an input method shows next to the text cursor.
    fn new_popup(&mut self, surface: ImePopupSurface) {
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("input-method popup not shown: {err}");
        }
    }

    fn popup_repositioned(&mut self, _surface: ImePopupSurface) {}

    fn dismiss_popup(&mut self, surface: ImePopupSurface) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.index_of(parent)
            .map(|index| self.windows[index].window.geometry())
            .unwrap_or_default()
    }
}
smithay::delegate_input_method_manager!(ArkaWm);
smithay::delegate_text_input_manager!(ArkaWm);
smithay::delegate_virtual_keyboard_manager!(ArkaWm);
