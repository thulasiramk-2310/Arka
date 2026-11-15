//! What goes on screen each frame, shared by both ways ArkaWM can run: as a window inside another
//! desktop (`winit.rs`) and directly on the hardware (`udev.rs`).

use std::time::Duration;

use smithay::{
    backend::renderer::{
        damage::OutputDamageTracker,
        element::{
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            render_elements,
            solid::SolidColorRenderElement,
            surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
            AsRenderElements, Kind,
        },
        gles::GlesRenderer,
    },
    desktop::layer_map_for_output,
    output::Output,
    reexports::wayland_server::Resource,
    utils::{Physical, Point, Rectangle, Scale, Size, Transform},
    wayland::shell::wlr_layer::Layer,
};

use crate::ArkaWm;

/// Dark blue-black shown until the desktop shell has drawn its wallpaper, so start-up does not flash.
pub const BACKGROUND: [f32; 4] = [0.043, 0.059, 0.094, 1.0];
/// Behind the lock screen, and all there is if the locker has not drawn yet.
pub const LOCK_BACKGROUND: [f32; 4] = [0.047, 0.051, 0.063, 1.0];
/// ArkaOS green (#16c784) at 25%, premultiplied.
const SNAP_PREVIEW: [f32; 4] = [0.086 * 0.25, 0.78 * 0.25, 0.518 * 0.25, 0.25];

/// Black at 45%, premultiplied: the dimming around a part of the screen being picked.
const REGION_SHADE: [f32; 4] = [0.0, 0.0, 0.0, 0.45];

render_elements! {
    /// Everything ArkaWM draws.
    pub OverlayElement<=GlesRenderer>;
    Solid=SolidColorRenderElement,
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Memory=MemoryRenderBufferRenderElement<GlesRenderer>,
}

/// One frame, topmost element first.
pub struct Frame {
    pub elements: Vec<OverlayElement>,
    pub clear: [f32; 4],
    /// This is a lock-screen frame: no window is in it.
    pub locked: bool,
}

/// Build the frame. `pointer` draws ArkaWM's own pointer (on hardware; nested, the outer desktop draws it).
pub fn frame(state: &mut ArkaWm, renderer: &mut GlesRenderer, output: &Output, pointer: bool) -> Frame {
    let scale = state.scale;
    let origin = state.output_area().loc;
    let mut elements: Vec<OverlayElement> = Vec::new();

    if pointer {
        elements.extend(pointer_elements(state, renderer, origin));
    }

    // Locked: the lock screen on a plain background and nothing else.
    if state.locked {
        if let Some(surface) = state.lock_surface.as_ref().map(|s| s.wl_surface().clone()) {
            elements.extend(render_elements_from_surface_tree(renderer, &surface, (0, 0), scale, 1.0, Kind::Unspecified));
        }
        return Frame { elements, clear: LOCK_BACKGROUND, locked: true };
    }

    // Picking a part of the screen: everything outside the picked part is dimmed.
    if let Some(picked) = state.region.as_ref().map(|region| region.rect()) {
        let area = state.output_area();
        let picked = picked.map(|r| Rectangle::new(r.loc - area.loc, r.size));
        let bands = crate::capture::bands(area.size, picked);
        for (band, buffer) in bands.iter().zip(state.region_buffers.iter_mut()) {
            buffer.update(band.size, REGION_SHADE);
            elements.push(OverlayElement::Solid(SolidColorRenderElement::from_buffer(
                buffer,
                band.loc.to_physical_precise_round(scale),
                scale,
                1.0,
                Kind::Unspecified,
            )));
        }
    }

    // The Alt+Tab switcher, in the middle of the screen above everything else.
    state.update_switcher();
    if let Some((_, picture)) = state.switcher.as_ref().and_then(|s| s.picture.as_ref()) {
        let size = crate::switcher::size(state.switcher.as_ref().map(|s| s.order.len()).unwrap_or(0));
        let area = state.output_area().size;
        let place: Point<f64, Physical> = Point::<i32, smithay::utils::Logical>::from(((area.w - size.w) / 2, (area.h - size.h) / 2)).to_f64().to_physical(scale);
        match MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            place,
            picture,
            None,
            Some(Rectangle::from_size((size.w as f64 * scale, size.h as f64 * scale).into())),
            Some(size),
            Kind::Unspecified,
        ) {
            Ok(element) => elements.push(OverlayElement::Memory(element)),
            Err(err) => tracing::warn!("window switcher not drawn: {err:?}"),
        }
    }

    // While a window is dragged to a screen edge, show where it will snap.
    if let Some(rect) = state.snap_preview {
        state.snap_preview_buffer.update(rect.size, SNAP_PREVIEW);
        elements.push(OverlayElement::Solid(SolidColorRenderElement::from_buffer(
            &state.snap_preview_buffer,
            (rect.loc - origin).to_physical_precise_round(scale),
            scale,
            1.0,
            Kind::Unspecified,
        )));
    }
    // A full-screen window is drawn again above everything, so it covers panels and the dock.
    if let Some(window) = state.fullscreen_window() {
        if let Some(loc) = state.space.element_location(&window) {
            let at = loc - window.geometry().loc - origin;
            elements.extend(window.render_elements::<OverlayElement>(
                renderer,
                at.to_physical_precise_round(scale),
                Scale::from(scale),
                1.0,
            ));
        }
    }

    // Then, top to bottom: panels and docks, windows with their title bars, wallpaper layers.
    let layers = layer_map_for_output(output);
    let draw_layers = |kinds: [Layer; 2], renderer: &mut GlesRenderer, elements: &mut Vec<OverlayElement>| {
        for kind in kinds {
            for layer in layers.layers_on(kind).rev() {
                let Some(geo) = layers.layer_geometry(layer) else { continue };
                elements.extend(layer.render_elements::<OverlayElement>(
                    renderer,
                    geo.loc.to_physical_precise_round(scale),
                    Scale::from(scale),
                    1.0,
                ));
            }
        }
    };
    draw_layers([Layer::Overlay, Layer::Top], renderer, &mut elements);

    state.update_bars();
    let windows: Vec<_> = state.space.elements().rev().cloned().collect();
    for window in windows {
        let Some(loc) = state.space.element_location(&window) else { continue };
        let at = loc - window.geometry().loc - origin;
        elements.extend(window.render_elements::<OverlayElement>(
            renderer,
            at.to_physical_precise_round(scale),
            Scale::from(scale),
            1.0,
        ));
        let Some(index) = state.index_of_window(&window) else { continue };
        let managed = &state.windows[index];
        if let (Some(rect), Some((_, buffer))) = (state.bar_rect(managed), managed.bar.as_ref()) {
            let place: Point<f64, Physical> = (rect.loc - origin).to_f64().to_physical(scale);
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                place,
                buffer,
                None,
                // the whole picture (drawn at screen resolution), shown at the bar's logical size
                Some(Rectangle::from_size((rect.size.w as f64 * scale, rect.size.h as f64 * scale).into())),
                Some(rect.size),
                Kind::Unspecified,
            ) {
                Ok(element) => elements.push(OverlayElement::Memory(element)),
                Err(err) => tracing::warn!("title bar not drawn: {err:?}"),
            }
        }
    }
    draw_layers([Layer::Bottom, Layer::Background], renderer, &mut elements);

    Frame { elements, clear: BACKGROUND, locked: false }
}

/// The pointer, as the app under it asked for: its own picture, a named shape from the cursor
/// theme, or nothing. Falls back to ArkaWM's built-in arrow when the theme has no such shape.
fn pointer_elements(state: &mut ArkaWm, renderer: &mut GlesRenderer, origin: Point<i32, smithay::utils::Logical>) -> Vec<OverlayElement> {
    use smithay::input::pointer::{CursorIcon, CursorImageStatus, CursorImageSurfaceData};
    let scale = state.scale;
    let at = state.seat.get_pointer().unwrap().current_location() - origin.to_f64();
    // An app that has gone away cannot keep its picture on the pointer.
    if matches!(&state.cursor_status, CursorImageStatus::Surface(surface) if !surface.is_alive()) {
        state.cursor_status = CursorImageStatus::default_named();
    }
    let icon = match state.cursor_status.clone() {
        CursorImageStatus::Hidden => return Vec::new(),
        CursorImageStatus::Surface(surface) => {
            let hotspot = smithay::wayland::compositor::with_states(&surface, |states| {
                states.data_map.get::<CursorImageSurfaceData>().map(|data| data.lock().unwrap().hotspot).unwrap_or_default()
            });
            let place: Point<i32, Physical> = (at - hotspot.to_f64()).to_physical_precise_round(scale);
            return render_elements_from_surface_tree(renderer, &surface, place, scale, 1.0, Kind::Cursor);
        }
        CursorImageStatus::Named(icon) => icon,
    };
    let picture = state.cursor_cache.entry(icon).or_insert_with(|| crate::cursor::load(icon, scale)).clone();
    // A shape the theme lacks falls back to the theme's ordinary arrow, then to the built-in one.
    let picture = picture.or_else(|| {
        state.cursor_cache.entry(CursorIcon::Default).or_insert_with(|| crate::cursor::load(CursorIcon::Default, scale)).clone()
    });
    let (buffer, pixels, size, hotspot) = match picture {
        Some(p) => (p.buffer, p.pixels, p.size, p.hotspot),
        None => (
            state.pointer_buffer.get_or_insert_with(pointer_picture).clone(),
            (POINTER.0 * 3, POINTER.1 * 3).into(),
            POINTER.into(),
            (0.0, 0.0).into(),
        ),
    };
    let place: Point<f64, Physical> = (at - hotspot).to_physical(scale);
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        place,
        &buffer,
        None,
        // the whole picture, shown at the pointer's logical size
        Some(Rectangle::from_size(pixels.to_f64())),
        Some(size),
        Kind::Cursor,
    )
    .map(|element| vec![OverlayElement::Memory(element)])
    .unwrap_or_default()
}

/// After a frame is on its way to the screen: confirm a lock, tell apps to draw their next frame,
/// and tidy up.
pub fn after_frame(state: &mut ArkaWm, output: &Output, frame_was_locked: bool) {
    // Only now, with a window-free frame drawn, is the locker told the session is locked.
    if frame_was_locked {
        if let Some(confirmation) = state.lock_pending.take() {
            confirmation.lock();
            tracing::info!("screen locked");
        }
    }
    let elapsed = state.start_time.elapsed();
    if let Some(surface) = state.lock_surface.as_ref().map(|s| s.wl_surface().clone()) {
        smithay::desktop::utils::send_frames_surface_tree(&surface, output, elapsed, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }
    // A video that has closed no longer keeps the screen awake.
    if !state.idle_inhibitors.is_empty() {
        state.idle_inhibitors.retain(|s| s.is_alive());
        let held = !state.idle_inhibitors.is_empty();
        state.idle_notifier_state.set_is_inhibited(held);
    }
    state.space.elements().for_each(|window| {
        window.send_frame(output, elapsed, Some(Duration::ZERO), |_, _| Some(output.clone()))
    });
    for layer in layer_map_for_output(output).layers() {
        layer.send_frame(output, elapsed, Some(Duration::ZERO), |_, _| Some(output.clone()));
    }
    state.space.refresh();
    state.popups.cleanup();
    let _ = state.display_handle.clone().flush_clients();
}

/// Developer test hook: draw the frame once more into a texture and save it as a PPM file.
/// (Reading back from the real target can break it.) `flipped` when the output is drawn upside down.
pub fn save_shot(
    renderer: &mut GlesRenderer,
    output: &Output,
    size: Size<i32, Physical>,
    frame: &Frame,
    path: &std::path::Path,
    flipped: bool,
) {
    if let Some(rgba) = grab(renderer, output, size, frame, flipped) {
        let mut out = format!("P6\n{} {}\n255\n", size.w, size.h).into_bytes();
        out.extend(rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]));
        let _ = std::fs::write(path, out);
    }
}

/// Draw a frame off screen and return its pixels: RGBA, top row first.
pub fn grab(
    renderer: &mut GlesRenderer,
    output: &Output,
    size: Size<i32, Physical>,
    frame: &Frame,
    flipped: bool,
) -> Option<Vec<u8>> {
    use smithay::backend::renderer::{gles::GlesTexture, Bind, ExportMem, Offscreen};
    let format = smithay::backend::allocator::Fourcc::Abgr8888;
    let saved = (|| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut texture: GlesTexture = renderer.create_buffer(format, (size.w, size.h).into())?;
        let mut target = renderer.bind(&mut texture)?;
        let mut tracker = OutputDamageTracker::from_output(output);
        tracker.render_output(renderer, &mut target, 0, &frame.elements, frame.clear).map_err(|e| format!("{e:?}"))?;
        let mapping = renderer.copy_framebuffer(&target, Rectangle::from_size((size.w, size.h).into()), format)?;
        Ok(renderer.map_texture(&mapping)?.to_vec())
    })();
    match saved {
        Ok(bytes) => {
            let rows = bytes.chunks_exact(size.w as usize * 4);
            let rows: Vec<&[u8]> = if flipped { rows.rev().collect() } else { rows.collect() };
            Some(rows.concat())
        }
        Err(err) => {
            tracing::warn!("screenshot failed: {err}");
            None
        }
    }
}

/// Size of ArkaWM's own pointer, in logical pixels.
const POINTER: (i32, i32) = (13, 21);
/// The arrow's outline: tip at the top left, like the usual pointer.
const ARROW: [(f32, f32); 7] = [(0.5, 0.5), (0.5, 16.5), (4.5, 12.8), (7.6, 19.8), (10.0, 18.7), (7.0, 11.8), (12.2, 11.8)];

/// Whether a point is inside the arrow (even-odd rule).
fn in_arrow(x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = ARROW.len() - 1;
    for i in 0..ARROW.len() {
        let (xi, yi) = ARROW[i];
        let (xj, yj) = ARROW[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// ArkaWM's own pointer for running on hardware: a white arrow with a dark edge, drawn at three
/// times its size so it stays sharp on dense screens. A placeholder until apps' own cursor
/// pictures and a cursor theme are wired in.
pub fn pointer_picture() -> MemoryRenderBuffer {
    const K: i32 = 3;
    let (w, h) = (POINTER.0 * K, POINTER.1 * K);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for py in 0..h {
        for px in 0..w {
            let (x, y) = ((px as f32 + 0.5) / K as f32, (py as f32 + 0.5) / K as f32);
            if !in_arrow(x, y) {
                continue;
            }
            // dark where the edge is less than about one pixel away
            let edge = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)].iter().any(|(dx, dy)| !in_arrow(x + dx, y + dy));
            let value = if edge { 0x18 } else { 0xFF };
            let i = ((py * w + px) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[value, value, value, 0xFF]);
        }
    }
    MemoryRenderBuffer::from_slice(&pixels, smithay::backend::allocator::Fourcc::Argb8888, (w, h), 1, Transform::Normal, None)
}

#[cfg(test)]
mod tests {
    use super::in_arrow;

    #[test]
    fn the_pointer_is_an_arrow_with_its_tip_at_the_top_left() {
        assert!(in_arrow(1.0, 2.0));
        assert!(in_arrow(3.0, 8.0));
        assert!(!in_arrow(8.0, 2.0));
        assert!(!in_arrow(1.0, 19.0));
        assert!(in_arrow(8.0, 17.0));
    }
}
