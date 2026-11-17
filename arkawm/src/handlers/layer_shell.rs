//! wlr-layer-shell: panels, docks and overlays (arka-shell's bar, Crystal Dock,
//! the privacy dots). Space they reserve is taken out of the window row.

use smithay::{
    delegate_layer_shell,
    desktop::{layer_map_for_output, LayerSurface, WindowSurfaceType},
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    wayland::{
        compositor::with_states,
        shell::wlr_layer::{
            Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler,
            WlrLayerShellState,
        },
    },
};

use crate::ArkaWm;

impl WlrLayerShellHandler for ArkaWm {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        if matches!(layer, Layer::Top | Layer::Overlay) {
            self.fresh_layers.push(surface.wl_surface().clone());
        }
        let Some(output) = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.space.outputs().next().cloned())
        else {
            return;
        };
        let mut map = layer_map_for_output(&output);
        if let Err(err) = map.map_layer(&LayerSurface::new(surface, namespace)) {
            tracing::warn!("could not map layer surface: {err}");
        }
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        self.fresh_layers.retain(|s| s != surface.wl_surface());
        let keyboard = self.seat.get_keyboard().unwrap();
        let had_keys = keyboard.current_focus().is_none_or(|focus| &focus == surface.wl_surface());
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();
        for output in outputs {
            let mut map = layer_map_for_output(&output);
            let layer = map.layers().find(|l| l.layer_surface() == &surface).cloned();
            if let Some(layer) = layer {
                map.unmap_layer(&layer);
            }
        }
        self.arrange();
        // A sheet that had the keyboard closed: the window that had it before gets it back.
        if had_keys && !self.locked {
            if self.windows.get(self.focused).is_some_and(|m| !m.minimized) {
                self.focus_index(self.focused);
            } else if let Some(desktop) = self.desktop_surface() {
                self.focus_surface(Some(desktop), smithay::utils::SERIAL_COUNTER.next_serial());
            }
        }
        self.refresh_pointer_focus();
    }
}

delegate_layer_shell!(ArkaWm);

impl ArkaWm {
    /// Arrange layer surfaces on commit and send the first configure.
    pub fn handle_layer_commit(&mut self, surface: &WlSurface) {
        let Some(output) = self
            .space
            .outputs()
            .find(|o| {
                layer_map_for_output(o)
                    .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                    .is_some()
            })
            .cloned()
        else {
            return;
        };
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .map(|d| d.lock().unwrap().initial_configure_sent)
                .unwrap_or(true)
        });
        {
            let mut map = layer_map_for_output(&output);
            map.arrange();
            if !initial_configure_sent {
                if let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL) {
                    layer.layer_surface().send_configure();
                }
            }
        }
        // A panel may have reserved or released space.
        self.arrange();
        // A sheet that was just opened (by a shortcut as well as by a click) and asks for keys
        // gets the keyboard as soon as it is on screen, so Super+S and typing finds an app.
        if let Some(at) = self.fresh_layers.iter().position(|s| s == surface) {
            let drawn = smithay::backend::renderer::utils::with_renderer_surface_state(surface, |state| state.buffer().is_some())
                .unwrap_or(true);
            if drawn {
                self.fresh_layers.remove(at);
                use smithay::wayland::shell::wlr_layer::{KeyboardInteractivity, LayerSurfaceCachedState};
                let wants_keys = with_states(surface, |states| {
                    states.cached_state.get::<LayerSurfaceCachedState>().current().keyboard_interactivity
                        != KeyboardInteractivity::None
                });
                if wants_keys {
                    self.focus_surface(Some(surface.clone()), smithay::utils::SERIAL_COUNTER.next_serial());
                }
            }
        }
        // The desktop has just appeared (or changed) with no window on screen: give it the
        // keyboard, so the apps on it can be reached with Tab and Enter without a click first.
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.current_focus().is_none() && self.windows.iter().all(|m| m.minimized) {
            if let Some(desktop) = self.desktop_surface() {
                let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                self.focus_surface(Some(desktop), serial);
            }
        }
        self.refresh_pointer_focus();
    }

    /// A panel appeared, moved or went away under a pointer that is not moving: point the
    /// pointer at what is under it now, so that panel hears the pointer is over it (a strip
    /// that slides in under the pointer would otherwise think the pointer is elsewhere).
    pub fn refresh_pointer_focus(&mut self) {
        let pointer = self.seat.get_pointer().unwrap();
        if pointer.is_grabbed() || self.region.is_some() {
            return;
        }
        let location = pointer.current_location();
        let under = self.surface_under(location);
        if under.as_ref().map(|(surface, _)| surface) == pointer.current_focus().as_ref() {
            return;
        }
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        pointer.motion(self, under, &smithay::input::pointer::MotionEvent { location, serial, time: 0 });
        pointer.frame(self);
    }

    /// The desktop the shell draws under the windows: a background-layer surface that takes
    /// the keyboard. `None` when no shell runs or its desktop does not want keys.
    pub fn desktop_surface(&self) -> Option<WlSurface> {
        use smithay::wayland::shell::wlr_layer::{KeyboardInteractivity, LayerSurfaceCachedState};
        self.space.outputs().find_map(|output| {
            layer_map_for_output(output).layers_on(Layer::Background).find_map(|layer| {
                let surface = layer.wl_surface().clone();
                let wants_keys = with_states(&surface, |states| {
                    states.cached_state.get::<LayerSurfaceCachedState>().current().keyboard_interactivity
                        != KeyboardInteractivity::None
                });
                wants_keys.then_some(surface)
            })
        })
    }
}
