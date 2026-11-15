//! The window switcher: hold Alt and press Tab, as on Windows. A panel lists the open windows,
//! most recently used first; each Tab moves to the next (Shift+Tab to the previous); letting go
//! of Alt goes to the chosen window, Esc leaves things as they were.

use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    desktop::Window,
    utils::{Logical, Size, Transform},
};

use crate::state::ArkaWm;
use crate::titlebar::text;

const WIDTH: i32 = 520;
const ROW: i32 = 44;
const PAD: i32 = 10;
const RADIUS: f32 = 16.0;
const PANEL: [u8; 3] = [0x12, 0x16, 0x22];
const CHOSEN: [u8; 3] = [0x3A, 0x42, 0x58];
const INK: [u8; 3] = [0xF4, 0xF7, 0xFC];
const DIM: [u8; 3] = [0xB4, 0xBD, 0xCE];

pub struct Switcher {
    /// The windows, most recently used first.
    pub order: Vec<Window>,
    pub at: usize,
    /// The panel's picture and what it was drawn from.
    pub picture: Option<(Key, MemoryRenderBuffer)>,
}

#[derive(Clone, PartialEq)]
pub struct Key {
    pub titles: Vec<String>,
    pub at: usize,
    pub scale120: i32,
}

/// Where Tab goes from `at` among `len` windows.
pub fn step(at: usize, len: usize, back: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if back { (at + len - 1) % len } else { (at + 1) % len }
}

/// The panel's size in logical pixels for this many windows.
pub fn size(rows: usize) -> Size<i32, Logical> {
    (WIDTH, 2 * PAD + ROW * rows.max(1) as i32).into()
}

/// How much of a pixel at (x, y) lies inside a rounded rectangle, 0..1.
fn inside(x: f32, y: f32, left: f32, top: f32, w: f32, h: f32, r: f32) -> f32 {
    let dx = ((left + r) - x).max(x - (left + w - r)).max(0.0);
    let dy = ((top + r) - y).max(y - (top + h - r)).max(0.0);
    (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0)
}

/// Paint a rounded rectangle over premultiplied BGRA pixels.
fn fill(pixels: &mut [u8], w: i32, rect: (f32, f32, f32, f32), r: f32, colour: [u8; 3], alpha: f32) {
    let (left, top, width, height) = rect;
    for y in top.floor() as i32..(top + height).ceil() as i32 {
        for x in left.floor() as i32..(left + width).ceil() as i32 {
            let a = inside(x as f32 + 0.5, y as f32 + 0.5, left, top, width, height, r) * alpha;
            if a <= 0.0 || x < 0 || x >= w {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            let Some(pixel) = pixels.get_mut(i..i + 4) else { continue };
            for (channel, value) in [colour[2], colour[1], colour[0], 255].into_iter().enumerate() {
                pixel[channel] = (value as f32 * a + pixel[channel] as f32 * (1.0 - a)).round() as u8;
            }
        }
    }
}

pub fn draw(key: &Key) -> MemoryRenderBuffer {
    let scale = key.scale120 as f32 / 120.0;
    let logical = size(key.titles.len());
    let (w, h) = ((logical.w as f32 * scale).round() as i32, (logical.h as f32 * scale).round() as i32);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    fill(&mut pixels, w, (0.0, 0.0, w as f32, h as f32), RADIUS * scale, PANEL, 0.95);
    let (pad, row) = (PAD as f32 * scale, ROW as f32 * scale);
    for (i, title) in key.titles.iter().enumerate() {
        let top = pad + row * i as f32;
        if i == key.at {
            fill(&mut pixels, w, (pad, top, w as f32 - 2.0 * pad, row), 10.0 * scale, CHOSEN, 1.0);
        }
        // The text helper centres on a strip's height, so hand it this row only.
        let (from, to) = ((top as i32).clamp(0, h), ((top + row) as i32).clamp(0, h));
        let strip = &mut pixels[(from * w * 4) as usize..(to * w * 4) as usize];
        let title = if title.is_empty() { "Untitled window" } else { title.as_str() };
        let ink = if i == key.at { INK } else { DIM };
        text(strip, w, to - from, pad + 14.0 * scale, w as f32 - pad - 14.0 * scale, 14.0 * scale, ink, title, true);
    }
    MemoryRenderBuffer::from_slice(&pixels, Fourcc::Argb8888, (w, h), 1, Transform::Normal, None)
}

impl ArkaWm {
    pub fn window_title(window: &Window) -> String {
        let Some(toplevel) = window.toplevel() else { return String::new() };
        smithay::wayland::compositor::with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().unwrap().title.clone())
                .unwrap_or_default()
        })
    }

    /// Alt+Tab: open the switcher on the previous window, or move on if it is open.
    pub fn switcher_step(&mut self, back: bool) {
        match self.switcher.as_mut() {
            Some(switcher) => switcher.at = step(switcher.at, switcher.order.len(), back),
            None => {
                // The stacking order is the order of use: the top window was used last.
                let mut order: Vec<Window> = self.space.elements().rev().filter(|w| self.index_of_window(w).is_some()).cloned().collect();
                // then the windows that are put away
                order.extend(self.windows.iter().filter(|m| m.minimized).map(|m| m.window.clone()));
                if order.is_empty() {
                    return;
                }
                let at = step(0, order.len(), back);
                self.switcher = Some(Switcher { order, at, picture: None });
            }
        }
        self.redraw_wanted = true;
    }

    /// Alt was let go (`go`), or Esc was pressed: close the switcher.
    pub fn switcher_close(&mut self, go: bool) {
        let Some(switcher) = self.switcher.take() else { return };
        self.redraw_wanted = true;
        if !go {
            return;
        }
        // The window may have closed while the switcher was open.
        if let Some(index) = switcher.order.get(switcher.at).and_then(|w| self.index_of_window(w)) {
            self.focus_index(index);
        }
    }

    /// Bring the switcher's picture up to date before a frame.
    pub fn update_switcher(&mut self) {
        let scale120 = (self.scale * 120.0).round() as i32;
        let Some(switcher) = self.switcher.as_mut() else { return };
        let key = Key { titles: switcher.order.iter().map(Self::window_title).collect(), at: switcher.at, scale120 };
        if switcher.picture.as_ref().map(|(k, _)| k) != Some(&key) {
            let picture = draw(&key);
            switcher.picture = Some((key, picture));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_goes_round() {
        assert_eq!(step(0, 3, false), 1);
        assert_eq!(step(2, 3, false), 0);
        assert_eq!(step(0, 3, true), 2);
        assert_eq!(step(0, 1, false), 0);
        assert_eq!(step(0, 0, false), 0);
    }

    #[test]
    fn the_panel_is_drawn_at_the_scaled_size() {
        let key = Key { titles: vec!["Files".into(), String::new(), "A very long title ".repeat(12)], at: 1, scale120: 240 };
        let _ = draw(&key);
        assert_eq!(size(3).h, 2 * PAD + 3 * ROW);
        assert_eq!(size(0).h, 2 * PAD + ROW);
    }
}
