use crate::{state::ClientState, ArkaWm};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler, CompositorState,
        },
        shm::{ShmHandler, ShmState},
    },
};

use super::xdg_shell;

impl CompositorHandler for ArkaWm {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        // Smithay 0.7 keeps checking a panel's size after the panel itself is gone, and its
        // record of that size is empty by then. A program that closes a panel and then clears
        // the surface (GTK does) would be thrown out for "width 0". This runs before that check
        // and gives the record a size, which nothing reads once the panel is gone.
        smithay::wayland::compositor::add_pre_commit_hook::<Self, _>(surface, |state, _, surface| {
            use smithay::wayland::shell::wlr_layer::{LayerSurfaceCachedState, LayerSurfaceData};
            let was_panel = smithay::wayland::compositor::with_states(surface, |states| {
                states.data_map.get::<LayerSurfaceData>().is_some()
            });
            if !was_panel || state.layer_shell_state.layer_surfaces().any(|layer| layer.wl_surface() == surface) {
                return;
            }
            smithay::wayland::compositor::with_states(surface, |states| {
                states.cached_state.get::<LayerSurfaceCachedState>().pending().size = (1, 1).into();
            });
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().unwrap().wl_surface() == &root)
            {
                window.on_commit();
            }
        };

        xdg_shell::handle_commit(&mut self.popups, &self.space, surface);
        self.place_new_window(surface);
        self.handle_layer_commit(surface);
    }
}

impl BufferHandler for ArkaWm {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for ArkaWm {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(ArkaWm);
delegate_shm!(ArkaWm);
