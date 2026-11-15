//! Screenshots: the whole screen (Print Screen) or a part picked by dragging (Shift+Print Screen,
//! or the Windows key with Shift+S). The picture is saved under Pictures/Screenshots and put on
//! the clipboard.

use smithay::utils::{Logical, Physical, Point, Rectangle, Size};

use crate::state::ArkaWm;

/// A screenshot to take on the next frame.
pub struct Capture {
    /// The part of the screen to keep; the whole screen if none.
    pub crop: Option<Rectangle<i32, Logical>>,
}

/// Picking a part of the screen: the corner where the drag began (once it has), and the pointer.
#[derive(Default)]
pub struct Region {
    pub start: Option<Point<f64, Logical>>,
    pub now: Point<f64, Logical>,
}

impl Region {
    pub fn rect(&self) -> Option<Rectangle<i32, Logical>> {
        let start = self.start?;
        let (x0, x1) = (start.x.min(self.now.x), start.x.max(self.now.x));
        let (y0, y1) = (start.y.min(self.now.y), start.y.max(self.now.y));
        Some(Rectangle::new((x0.round() as i32, y0.round() as i32).into(), ((x1 - x0).round() as i32, (y1 - y0).round() as i32).into()))
    }
}

/// The four dark bands around a picked part of a screen of this size (the whole screen if nothing
/// is picked yet).
pub fn bands(screen: Size<i32, Logical>, picked: Option<Rectangle<i32, Logical>>) -> Vec<Rectangle<i32, Logical>> {
    let Some(p) = picked else { return vec![Rectangle::from_size(screen)] };
    let p = p.intersection(Rectangle::from_size(screen)).unwrap_or_default();
    [
        Rectangle::new((0, 0).into(), (screen.w, p.loc.y).into()),
        Rectangle::new((0, p.loc.y + p.size.h).into(), (screen.w, screen.h - p.loc.y - p.size.h).into()),
        Rectangle::new((0, p.loc.y).into(), (p.loc.x, p.size.h).into()),
        Rectangle::new((p.loc.x + p.size.w, p.loc.y).into(), (screen.w - p.loc.x - p.size.w, p.size.h).into()),
    ]
    .into_iter()
    .filter(|r| r.size.w > 0 && r.size.h > 0)
    .collect()
}

/// Cut `crop` (physical pixels) out of RGBA rows of a picture `size` big.
pub fn cut(rgba: &[u8], size: Size<i32, Physical>, crop: Rectangle<i32, Physical>) -> (Vec<u8>, Size<i32, Physical>) {
    let crop = crop.intersection(Rectangle::from_size(size)).unwrap_or_default();
    let mut out = Vec::with_capacity((crop.size.w * crop.size.h * 4) as usize);
    for y in crop.loc.y..crop.loc.y + crop.size.h {
        let from = ((y * size.w + crop.loc.x) * 4) as usize;
        out.extend_from_slice(&rgba[from..from + (crop.size.w * 4) as usize]);
    }
    (out, crop.size)
}

fn write_png(path: &std::path::Path, rgba: &[u8], size: Size<i32, Physical>) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, size.w as u32, size.h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

impl ArkaWm {
    /// Save a grabbed screen (RGBA, top row first) as this capture asks, copy it to the clipboard
    /// and tell the shell.
    pub fn finish_capture(&mut self, capture: Capture, rgba: Vec<u8>, size: Size<i32, Physical>) {
        let (mut rgba, size) = match capture.crop {
            Some(crop) => {
                let origin = self.output_area().loc;
                let crop = Rectangle::new(crop.loc - origin, crop.size).to_f64().to_physical(self.scale).to_i32_round();
                cut(&rgba, size, crop)
            }
            None => (rgba, size),
        };
        if size.w <= 0 || size.h <= 0 {
            return;
        }
        // the screen has no see-through parts
        rgba.chunks_exact_mut(4).for_each(|pixel| pixel[3] = 255);
        let Some(home) = std::env::var_os("HOME") else { return };
        let dir = std::path::Path::new(&home).join("Pictures/Screenshots");
        let stamp = std::process::Command::new("date")
            .arg("+%Y-%m-%d %H-%M-%S")
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_else(|_| self.start_time.elapsed().as_millis().to_string());
        let path = dir.join(format!("Screenshot {stamp}.png"));
        match std::fs::create_dir_all(&dir).map_err(Into::into).and_then(|_| write_png(&path, &rgba, size)) {
            Ok(()) => {
                // On the clipboard too, as on Windows. wl-copy keeps running to serve the picture.
                if let Ok(file) = std::fs::File::open(&path) {
                    let _ = std::process::Command::new("wl-copy").args(["--type", "image/png"]).stdin(file).spawn();
                }
                self.tell_shell(&format!("shot {}\n", path.display()));
                tracing::info!("screenshot saved: {}", path.display());
            }
            Err(err) => {
                tracing::warn!("screenshot not saved: {err}");
                self.tell_shell("shot-failed\n");
            }
        }
    }

    /// A mouse button while a part of the screen is being picked. Returns true if it was used.
    pub fn region_button(&mut self, pressed: bool, pos: Point<f64, Logical>) -> bool {
        let Some(region) = self.region.as_mut() else { return false };
        self.redraw_wanted = true;
        if pressed {
            region.start = Some(pos);
            region.now = pos;
        } else if let Some(rect) = self.region.take().and_then(|region| region.rect()) {
            // a click without a drag picks nothing
            if rect.size.w >= 4 && rect.size.h >= 4 {
                self.capture = Some(Capture { crop: Some(rect) });
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_in_any_direction_gives_the_same_part() {
        let region = Region { start: Some((300.0, 200.0).into()), now: (100.0, 50.0).into() };
        assert_eq!(region.rect(), Some(Rectangle::new((100, 50).into(), (200, 150).into())));
        assert_eq!(Region::default().rect(), None);
    }

    #[test]
    fn the_bands_surround_the_picked_part() {
        let screen: Size<i32, Logical> = (100, 80).into();
        assert_eq!(bands(screen, None), vec![Rectangle::from_size(screen)]);
        let around = bands(screen, Some(Rectangle::new((10, 20).into(), (30, 40).into())));
        let area: i32 = around.iter().map(|r| r.size.w * r.size.h).sum();
        assert_eq!(area, 100 * 80 - 30 * 40);
        // a part touching the top-left corner needs only two bands
        assert_eq!(bands(screen, Some(Rectangle::new((0, 0).into(), (30, 40).into()))).len(), 2);
    }

    #[test]
    fn cutting_keeps_the_right_pixels() {
        // 3x2 picture, each pixel's red channel is its number
        let rgba: Vec<u8> = (0..6u8).flat_map(|n| [n, 0, 0, 255]).collect();
        let (out, size) = cut(&rgba, (3, 2).into(), Rectangle::new((1, 0).into(), (2, 2).into()));
        assert_eq!((size.w, size.h), (2, 2));
        assert_eq!(out.iter().step_by(4).copied().collect::<Vec<_>>(), [1, 2, 4, 5]);
        // a part hanging off the edge is clipped
        let (_, size) = cut(&rgba, (3, 2).into(), Rectangle::new((2, 1).into(), (5, 5).into()));
        assert_eq!((size.w, size.h), (1, 1));
    }
}
