//! Pointer pictures from the installed cursor theme, for when ArkaWM draws the pointer itself.

use std::io::Read;

use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    input::pointer::CursorIcon,
    utils::{Logical, Point, Size, Transform},
};

/// One pointer picture, ready to draw.
#[derive(Clone)]
pub struct Picture {
    pub buffer: MemoryRenderBuffer,
    /// Size of the picture in its own pixels.
    pub pixels: Size<i32, Logical>,
    /// Size it is shown at, in logical pixels.
    pub size: Size<i32, Logical>,
    /// The point of the picture that sits on the pointer position, in logical pixels.
    pub hotspot: Point<f64, Logical>,
}

/// The pointer's nominal size in logical pixels: `XCURSOR_SIZE`, or 24.
fn nominal_size() -> u32 {
    std::env::var("XCURSOR_SIZE").ok().and_then(|v| v.parse().ok()).filter(|v| (8..=256).contains(v)).unwrap_or(24)
}

/// Of the sizes a cursor file offers, the one closest to what the screen needs.
pub fn best_size(available: &[u32], wanted: u32) -> Option<u32> {
    available.iter().copied().min_by_key(|size| size.abs_diff(wanted))
}

/// Load a pointer shape from the cursor theme (`XCURSOR_THEME`, or the system default), at the
/// sharpness the screen scale needs. `None` if the theme does not have it.
pub fn load(icon: CursorIcon, scale: f64) -> Option<Picture> {
    let theme = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
    let theme = xcursor::CursorTheme::load(&theme);
    let path = std::iter::once(icon.name()).chain(icon.alt_names().iter().copied()).find_map(|name| theme.load_icon(name))?;
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    let images = xcursor::parser::parse_xcursor(&bytes)?;

    let nominal = nominal_size();
    let sizes: Vec<u32> = images.iter().map(|image| image.size).collect();
    let chosen = best_size(&sizes, (nominal as f64 * scale).round() as u32)?;
    // An animated cursor has several pictures of one size; the first is used.
    let image = images.into_iter().find(|image| image.size == chosen)?;
    if image.width == 0 || image.height == 0 || image.pixels_rgba.len() != (image.width * image.height * 4) as usize {
        return None;
    }
    // How many picture pixels make one logical pixel.
    let density = image.size as f64 / nominal as f64;
    let pixels: Size<i32, Logical> = (image.width as i32, image.height as i32).into();
    Some(Picture {
        buffer: MemoryRenderBuffer::from_slice(&image.pixels_rgba, Fourcc::Abgr8888, (pixels.w, pixels.h), 1, Transform::Normal, None),
        pixels,
        size: ((image.width as f64 / density).round() as i32, (image.height as f64 / density).round() as i32).into(),
        hotspot: (image.xhot as f64 / density, image.yhot as f64 / density).into(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_closest_cursor_size_is_chosen() {
        assert_eq!(super::best_size(&[24, 32, 48, 64], 48), Some(48));
        assert_eq!(super::best_size(&[24, 32, 48, 64], 36), Some(32));
        assert_eq!(super::best_size(&[24], 96), Some(24));
        assert_eq!(super::best_size(&[], 24), None);
    }
}
