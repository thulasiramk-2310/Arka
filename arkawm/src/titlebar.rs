//! Title bars ArkaWM draws for apps that ask for one (xdg-decoration), such as KDE apps and X11
//! apps. A bar shows the window title and a close button; dragging it moves the window and a
//! double click maximises it. Apps that draw their own bar (GTK, kitty, Firefox) are left alone.
//!
//! The look here is a plain placeholder in ArkaOS's dark chrome colours until the shell design
//! decides it.

use std::sync::OnceLock;

use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Logical, Point, Rectangle, Transform},
};

/// Height of a title bar, in logical pixels.
pub const BAR: i32 = 32;

const FOCUSED: [u8; 3] = [0x1B, 0x1E, 0x22];
const UNFOCUSED: [u8; 3] = [0x2A, 0x2E, 0x33];
const INK: [u8; 3] = [0xE6, 0xE8, 0xEB];
const DIM: [u8; 3] = [0x9A, 0xA0, 0xA6];

/// What a bar was drawn from; it is redrawn only when one of these changes.
#[derive(Clone, PartialEq)]
pub struct BarKey {
    pub width: i32,
    pub title: String,
    pub focused: bool,
    pub scale120: i32,
}

/// The buttons at the right end of a bar, each a square: minimise, maximise, close.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Button {
    Minimize,
    Maximize,
    Close,
}

/// The button a point on the bar is on, if any.
pub fn button_at(bar: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<Button> {
    if !bar.to_f64().contains(pos) {
        return None;
    }
    let squares_from_right = ((bar.loc.x + bar.size.w) as f64 - pos.x) / BAR as f64;
    match squares_from_right {
        x if x <= 1.0 => Some(Button::Close),
        x if x <= 2.0 => Some(Button::Maximize),
        x if x <= 3.0 => Some(Button::Minimize),
        _ => None,
    }
}

/// Fill a rectangle given by its corners, clipped to the picture.
fn rect(pixels: &mut [u8], w: i32, h: i32, x0: f32, y0: f32, x1: f32, y1: f32, ink: [u8; 3]) {
    for y in (y0.round() as i32).max(0)..(y1.round() as i32).min(h) {
        for x in (x0.round() as i32).max(0)..(x1.round() as i32).min(w) {
            let i = ((y * w + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[ink[2], ink[1], ink[0], 0xFF]);
        }
    }
}

fn font() -> Option<&'static fontdue::Font> {
    static FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    FONT.get_or_init(|| {
        let named = std::env::var("ARKAWM_FONT").ok();
        let candidates = [
            "/usr/share/fonts/inter/Inter-Regular.ttf",
            "/usr/share/fonts/inter/Inter-Regular.otf",
            "/usr/share/fonts/google-noto-vf/NotoSans[wght].ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        ];
        let bytes = named
            .as_deref()
            .and_then(|path| std::fs::read(path).ok())
            .or_else(|| candidates.iter().find_map(|path| std::fs::read(path).ok()));
        bytes.and_then(|bytes| fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok())
    })
    .as_ref()
}

/// Blend one glyph coverage value onto a premultiplied BGRA pixel.
fn blend(pixel: &mut [u8], ink: [u8; 3], coverage: u8) {
    let a = coverage as u32;
    for (channel, value) in [ink[2], ink[1], ink[0]].into_iter().enumerate() {
        pixel[channel] = ((value as u32 * a + pixel[channel] as u32 * (255 - a)) / 255) as u8;
    }
}

/// Draw text with its left edge at `x`; returns the width used. Stops at `max_x`.
pub(crate) fn text(pixels: &mut [u8], w: i32, h: i32, mut x: f32, max_x: f32, px: f32, ink: [u8; 3], s: &str, draw: bool) -> f32 {
    let Some(font) = font() else { return 0.0 };
    let start = x;
    let baseline = (h as f32 + px * 0.7) / 2.0;
    for ch in s.chars() {
        let (m, bitmap) = font.rasterize(ch, px);
        if x + m.advance_width > max_x {
            break;
        }
        if draw {
            for row in 0..m.height {
                for col in 0..m.width {
                    let gx = x as i32 + m.xmin + col as i32;
                    let gy = baseline as i32 - m.ymin - m.height as i32 + row as i32;
                    if gx >= 0 && gx < w && gy >= 0 && gy < h {
                        let i = ((gy * w + gx) * 4) as usize;
                        blend(&mut pixels[i..i + 4], ink, bitmap[row * m.width + col]);
                    }
                }
            }
        }
        x += m.advance_width;
    }
    x - start
}

/// The picture of one title bar.
pub fn draw(key: &BarKey) -> MemoryRenderBuffer {
    let scale = key.scale120 as f32 / 120.0;
    let w = ((key.width as f32) * scale).round().max(1.0) as i32;
    let h = ((BAR as f32) * scale).round().max(1.0) as i32;
    let bg = if key.focused { FOCUSED } else { UNFOCUSED };
    let ink = if key.focused { INK } else { DIM };
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[bg[2], bg[1], bg[0], 0xFF]);
    }
    let px = 13.0 * scale;
    let button = h as f32;
    // title, centred, never under the buttons
    let buttons = 3.0 * button;
    let room = (w as f32 - 2.0 * buttons).max(0.0);
    let width = text(&mut pixels, w, h, 0.0, room, px, ink, &key.title, false);
    let x = buttons + ((room - width) / 2.0).max(0.0);
    text(&mut pixels, w, h, x, w as f32 - buttons, px, ink, &key.title, true);
    // minimise: a short line; maximise: a square outline
    let (half, line, mid) = (5.0 * scale, (1.4 * scale).max(1.0), h as f32 / 2.0);
    let cx = w as f32 - 2.5 * button;
    rect(&mut pixels, w, h, cx - half, mid + half - line, cx + half, mid + half, ink);
    let cx = w as f32 - 1.5 * button;
    rect(&mut pixels, w, h, cx - half, mid - half, cx + half, mid - half + line, ink);
    rect(&mut pixels, w, h, cx - half, mid + half - line, cx + half, mid + half, ink);
    rect(&mut pixels, w, h, cx - half, mid - half, cx - half + line, mid + half, ink);
    rect(&mut pixels, w, h, cx + half - line, mid - half, cx + half, mid + half, ink);
    // close button
    let glyph = 17.0 * scale;
    let cross = text(&mut pixels, w, h, 0.0, f32::MAX, glyph, ink, "\u{00D7}", false);
    text(&mut pixels, w, h, w as f32 - button + (button - cross) / 2.0, w as f32, glyph, ink, "\u{00D7}", true);

    // The buffer is at 1:1 with the screen; it is placed with a logical size when drawn.
    MemoryRenderBuffer::from_slice(&pixels, Fourcc::Argb8888, (w, h), 1, Transform::Normal, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_are_at_the_right_end_of_the_bar() {
        let bar = Rectangle::new((100, 50).into(), (400, BAR).into());
        assert_eq!(button_at(bar, (490.0, 60.0).into()), Some(Button::Close));
        assert_eq!(button_at(bar, (468.5, 50.0).into()), Some(Button::Close));
        assert_eq!(button_at(bar, (467.0, 60.0).into()), Some(Button::Maximize));
        assert_eq!(button_at(bar, (420.0, 60.0).into()), Some(Button::Minimize));
        assert_eq!(button_at(bar, (300.0, 60.0).into()), None);
        // below the bar is the window, not a button
        assert_eq!(button_at(bar, (490.0, 90.0).into()), None);
    }

    #[test]
    fn a_bar_is_drawn_at_the_scaled_size() {
        let key = BarKey { width: 400, title: "Files".into(), focused: true, scale120: 180 };
        // must not panic with or without a font, and with a title too long for the bar
        let _ = draw(&key);
        let _ = draw(&BarKey { width: 40, title: "A very long title that cannot fit".into(), ..key });
    }
}
