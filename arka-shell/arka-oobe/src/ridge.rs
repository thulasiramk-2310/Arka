//! The setup's progress: the ArkaOS peaks drawn as one hairline along the bottom, with a
//! marker that travels from peak to peak as the steps change. The line behind the marker
//! is drawn in the text colour, so the progress is the brand shape itself.
//!
//! Drawing is a handful of cairo lines, so it stays cheap on the CPU-only renderer the
//! setup runs on.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

/// Peak heights in px at the nominal strip height; the last one, the summit, is the tallest.
const PEAKS: [f64; 6] = [14.0, 20.0, 26.0, 20.0, 30.0, 44.0];
pub const STRIP_HEIGHT: i32 = 84;

pub struct Ridge {
    pub area: gtk4::DrawingArea,
    pos: Rc<Cell<f64>>,
    anim: RefCell<Option<adw::TimedAnimation>>,
}

struct Shape {
    pts: Vec<(f64, f64)>,
    cum: Vec<f64>,       // path length at each vertex
    apex: Vec<usize>,    // vertex index of each peak
}

fn shape(w: f64, h: f64) -> Shape {
    let base = h - 8.0;
    let scale = (h / STRIP_HEIGHT as f64).max(0.5);
    let hs: Vec<f64> = PEAKS.iter().map(|p| p * scale).collect();
    let used: f64 = hs.iter().map(|p| 2.0 * p).sum();
    let flat = ((w - used) / (hs.len() as f64 + 1.0)).max(4.0);
    let (mut x, mut y) = (0.0, base);
    let mut pts = vec![(x, y)];
    let mut apex = Vec::new();
    for p in &hs {
        x += flat;
        pts.push((x, y));
        x += p;
        y -= p;
        pts.push((x, y));
        apex.push(pts.len() - 1);
        x += p;
        y += p;
        pts.push((x, y));
    }
    pts.push((w, base));
    let mut cum = vec![0.0];
    for i in 1..pts.len() {
        let (a, b) = (pts[i - 1], pts[i]);
        cum.push(cum[i - 1] + ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt());
    }
    Shape { pts, cum, apex }
}

impl Ridge {
    pub fn new(dark: Rc<Cell<bool>>) -> Rc<Self> {
        let area = gtk4::DrawingArea::new();
        area.set_content_height(STRIP_HEIGHT);
        area.set_hexpand(true);
        area.set_can_target(false);
        area.set_can_focus(false);
        let pos = Rc::new(Cell::<f64>::new(0.0));
        {
            let pos = pos.clone();
            area.set_draw_func(move |_, cr, w, h| {
                let (w, h) = (w as f64, h as f64);
                let (fg, line) = if dark.get() { ((0.945, 0.945, 0.945), (0.231, 0.243, 0.271)) } else { ((0.067, 0.067, 0.067), (0.816, 0.812, 0.792)) };
                let bg = if dark.get() { (0.063, 0.067, 0.078) } else { (0.953, 0.949, 0.933) };
                let s = shape(w, h);
                cr.set_line_join(gtk4::cairo::LineJoin::Miter);
                // the whole ridge, as a hairline
                cr.set_source_rgb(line.0, line.1, line.2);
                cr.set_line_width(1.2);
                cr.move_to(s.pts[0].0, s.pts[0].1);
                for p in &s.pts[1..] {
                    cr.line_to(p.0, p.1);
                }
                let _ = cr.stroke();
                // how far along the path the marker is
                let stop: f64 = pos.get().clamp(0.0, (s.apex.len() - 1) as f64);
                let (i, f) = (stop.floor() as usize, stop.fract());
                let l0 = s.cum[s.apex[i]];
                let l1 = s.cum[s.apex[(i + 1).min(s.apex.len() - 1)]];
                let len = l0 + (l1 - l0) * f;
                // the part already travelled, drawn in the text colour
                cr.set_source_rgb(fg.0, fg.1, fg.2);
                cr.set_line_width(2.2);
                cr.move_to(s.pts[0].0, s.pts[0].1);
                let mut marker = s.pts[0];
                for k in 1..s.pts.len() {
                    let seg = s.cum[k] - s.cum[k - 1];
                    if s.cum[k] <= len {
                        cr.line_to(s.pts[k].0, s.pts[k].1);
                        marker = s.pts[k];
                    } else {
                        let t = if seg > 0.0 { (len - s.cum[k - 1]) / seg } else { 0.0 };
                        let (a, b) = (s.pts[k - 1], s.pts[k]);
                        marker = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                        cr.line_to(marker.0, marker.1);
                        break;
                    }
                }
                let _ = cr.stroke();
                // the marker: a ring with a dot
                cr.set_source_rgb(bg.0, bg.1, bg.2);
                cr.arc(marker.0, marker.1, 7.5, 0.0, std::f64::consts::TAU);
                let _ = cr.fill_preserve();
                cr.set_source_rgb(fg.0, fg.1, fg.2);
                cr.set_line_width(1.6);
                let _ = cr.stroke();
                cr.arc(marker.0, marker.1, 2.8, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            });
        }
        Rc::new(Self { area, pos, anim: RefCell::new(None) })
    }

    /// Move the marker to a stop. With `animate` it travels over 760 ms, easing in and out.
    pub fn go_to(&self, stop: usize, animate: bool) {
        let to = stop.min(PEAKS.len() - 1) as f64;
        if let Some(a) = self.anim.borrow_mut().take() {
            a.pause();
        }
        if !animate {
            self.pos.set(to);
            self.area.queue_draw();
            return;
        }
        let (pos, area) = (self.pos.clone(), self.area.clone());
        let target = adw::CallbackAnimationTarget::new(move |v| {
            pos.set(v);
            area.queue_draw();
        });
        let a = adw::TimedAnimation::new(&self.area, self.pos.get(), to, 760, target);
        a.set_easing(adw::Easing::EaseInOutCubic);
        a.play();
        *self.anim.borrow_mut() = Some(a);
    }
}
