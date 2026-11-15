use smithay::{
    backend::renderer::ImportDma,
    backend::{
        egl::EGLDevice,
        renderer::{damage::OutputDamageTracker, gles::GlesRenderer},
        winit::{self, WinitEvent},
    },
    desktop::layer_map_for_output,
    output::{Mode, Output, PhysicalProperties, Scale as OutputScale, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Rectangle, Transform},
    wayland::dmabuf::{DmabufFeedbackBuilder, DmabufState},
};

use crate::ArkaWm;

pub fn init_winit(
    event_loop: &mut EventLoop<'static, ArkaWm>,
    state: &mut ArkaWm,
) -> Result<(), Box<dyn std::error::Error>> {
    let display_handle = &state.display_handle.clone();

    let (mut backend, winit) = winit::init::<GlesRenderer>()?;
    backend.window().set_title("ArkaWM");

    // GPU buffer sharing (linux-dmabuf), so apps that draw with the GPU
    // (Firefox, kitty, games) can show their windows. With the render node
    // known, clients get v4 feedback; without it, v3.
    let formats = backend.renderer().dmabuf_formats();
    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten());
    let mut dmabuf_state = DmabufState::new();
    let dmabuf_global = match render_node
        .and_then(|node| DmabufFeedbackBuilder::new(node.dev_id(), formats.clone()).build().ok())
    {
        Some(feedback) => dmabuf_state.create_global_with_default_feedback::<ArkaWm>(display_handle, &feedback),
        None => {
            tracing::warn!("no render node found; offering dmabuf v3 without feedback");
            dmabuf_state.create_global::<ArkaWm>(display_handle, formats)
        }
    };
    state.dmabuf = Some((dmabuf_state, dmabuf_global));

    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "ArkaOS".into(),
            model: "ArkaWM nested".into(),
        },
    );
    let _global = output.create_global::<ArkaWm>(display_handle);
    let scale = state.scale;
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        Some(OutputScale::Fractional(scale)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);
    state.backend = Some(backend);

    std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);

    event_loop.handle().insert_source(winit, move |event, _, state| {

        match event {
            WinitEvent::Resized { size, .. } => {
                output.change_current_state(
                    Some(Mode {
                        size,
                        refresh: 60_000,
                    }),
                    None,
                    None,
                    None,
                );
                layer_map_for_output(&output).arrange();
                state.arrange();
            }
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => {
                let shot = state.shot_request.take();
                let capture = state.capture.take();
                // Nested, the outer desktop draws the pointer; `ARKAWM_DRAW_POINTER=1` draws ours too, for testing.
                let draw_pointer = std::env::var_os("ARKAWM_DRAW_POINTER").is_some();
                // The window is taken out of the state while drawing, so both can be used at once.
                let Some(mut backend) = state.backend.take() else { return };
                let size = backend.window_size();
                let damage = Rectangle::from_size(size);
                let mut drawn_locked = false;

                // A frame that fails to draw is skipped, never fatal: a
                // compositor crash would close every app.
                match backend.bind() {
                    Ok((renderer, mut framebuffer)) => {
                        let frame = crate::render::frame(state, renderer, &output, draw_pointer);
                        if let Some(path) = shot {
                            crate::render::save_shot(renderer, &output, size, &frame, &path, true);
                        }
                        // A screenshot for the user never has the pointer in it.
                        if let Some(capture) = capture {
                            tracing::debug!("taking a screenshot");
                            let clean = crate::render::frame(state, renderer, &output, false);
                            if let Some(rgba) = crate::render::grab(renderer, &output, size, &clean, true) {
                                state.finish_capture(capture, rgba, size);
                            }
                        }
                        match damage_tracker.render_output(renderer, &mut framebuffer, 0, &frame.elements, frame.clear) {
                            Ok(_) => drawn_locked = frame.locked,
                            Err(err) => tracing::warn!("frame not drawn: {err:?}"),
                        }
                    }
                    Err(err) => tracing::warn!("could not bind the window: {err:?}"),
                }
                if let Err(err) = backend.submit(Some(&[damage])) {
                    tracing::warn!("frame not shown: {err:?}");
                }
                backend.window().request_redraw();
                state.backend = Some(backend);

                crate::render::after_frame(state, &output, drawn_locked);
            }
            WinitEvent::CloseRequested => {
                state.loop_signal.stop();
            }
            _ => (),
        };
    })?;

    Ok(())
}
