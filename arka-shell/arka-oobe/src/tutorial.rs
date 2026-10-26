//! The format-wheel tutorial: a small practice window drawn inside the setup, so the wheel
//! always stays inside the frame it belongs to. Three moves, the same ones the real menu
//! asks for: click a file, choose "Convert with the format wheel", pick a format. Nothing
//! here touches a real file.
//!
//! It is a plain cairo drawing, so it runs on the CPU-only renderer the setup uses.

use std::cell::{Cell, RefCell};
use std::f64::consts::{PI, TAU};
use std::rc::Rc;
use std::time::Instant;

use gtk4::cairo;
use gtk4::glib;
use gtk4::prelude::*;

const W: f64 = 560.0;
const H: f64 = 290.0;
const BAR: f64 = 26.0;
const RING_R: f64 = 108.0;
const HUB_R: f64 = 36.0;
const SLICES: [&str; 3] = ["PNG", "WEBP", "PDF"];
const MENU: [&str; 10] = [
    "Open", "Copy", "Move to Trash", "-", "Convert to", "Edit picture", "Edit with the edit wheel",
    "Convert with the format wheel", "-", "Properties",
];
const WHEEL_ITEM: usize = 7;
const TILES: [(&str, Kind); 6] = [
    ("holiday.jpg", Kind::Image), ("notes.pdf", Kind::Doc), ("track.mp3", Kind::Audio),
    ("scan.png", Kind::Image), ("receipt.pdf", Kind::Doc), ("mix.mp3", Kind::Audio),
];

#[derive(Clone, Copy, PartialEq)]
enum Kind { Image, Doc, Audio }

#[derive(Clone, Copy, PartialEq)]
pub enum Stage { Click, Menu, Wheel, Done }

impl Stage {
    /// 1 to 4, for the step list beside the practice window.
    pub fn number(self) -> u8 {
        match self { Stage::Click => 1, Stage::Menu => 2, Stage::Wheel => 3, Stage::Done => 4 }
    }
}

struct State {
    stage: Stage,
    since: Instant,
    hover: Option<usize>,
    picked: Option<usize>,
}

struct Palette {
    fg: (f64, f64, f64),
    bg: (f64, f64, f64),
    panel: (f64, f64, f64),
    muted: (f64, f64, f64),
    line: (f64, f64, f64),
    lift: (f64, f64, f64),
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette { fg: (0.945, 0.945, 0.945), bg: (0.063, 0.067, 0.078), panel: (0.098, 0.104, 0.118),
                  muted: (0.65, 0.65, 0.65), line: (0.231, 0.243, 0.271), lift: (0.19, 0.20, 0.23) }
    } else {
        Palette { fg: (0.067, 0.067, 0.067), bg: (0.953, 0.949, 0.933), panel: (0.975, 0.972, 0.962),
                  muted: (0.36, 0.36, 0.36), line: (0.816, 0.812, 0.792), lift: (1.0, 1.0, 1.0) }
    }
}

fn tile_rect(i: usize) -> (f64, f64, f64, f64) {
    (16.0 + (i % 4) as f64 * 134.0, BAR + 18.0 + (i / 4) as f64 * 100.0, 122.0, 86.0)
}

fn menu_items() -> Vec<(f64, f64, usize)> {
    // (top, height, index) for each row, separators included
    let mut y = BAR + 8.0;
    let mut v = Vec::new();
    for (i, name) in MENU.iter().enumerate() {
        let h = if *name == "-" { 9.0 } else { 22.0 };
        v.push((y, h, i));
        y += h;
    }
    v
}
const MENU_X: f64 = 96.0;
const MENU_W: f64 = 216.0;

fn ring_center() -> (f64, f64) { (W / 2.0, BAR + (H - BAR) / 2.0) }

fn slice_at(x: f64, y: f64) -> Option<usize> {
    let (cx, cy) = ring_center();
    let (dx, dy) = (x - cx, y - cy);
    let d = (dx * dx + dy * dy).sqrt();
    if d < HUB_R || d > RING_R {
        return None;
    }
    let a = dy.atan2(dx).to_degrees();
    Some((((a + 150.0).rem_euclid(360.0)) / 120.0).floor() as usize % 3)
}

fn in_rect(x: f64, y: f64, r: (f64, f64, f64, f64)) -> bool {
    x >= r.0 && x <= r.0 + r.2 && y >= r.1 && y <= r.1 + r.3
}

fn text(cr: &cairo::Context, s: &str, x: f64, y: f64, size: f64, c: (f64, f64, f64), a: f64, bold: bool, center: bool) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, if bold { cairo::FontWeight::Bold } else { cairo::FontWeight::Normal });
    cr.set_font_size(size);
    cr.set_source_rgba(c.0, c.1, c.2, a);
    let ext = cr.text_extents(s).ok();
    let w = ext.map(|e| e.width()).unwrap_or(0.0);
    cr.move_to(if center { x - w / 2.0 } else { x }, y);
    let _ = cr.show_text(s);
}

fn icon(cr: &cairo::Context, kind: Kind, cx: f64, cy: f64, c: (f64, f64, f64)) {
    cr.set_source_rgba(c.0, c.1, c.2, 1.0);
    cr.set_line_width(1.3);
    match kind {
        Kind::Image => {
            cr.rectangle(cx - 17.0, cy - 14.0, 34.0, 28.0);
            let _ = cr.stroke();
            cr.move_to(cx - 17.0, cy + 8.0);
            cr.line_to(cx - 7.0, cy - 2.0);
            cr.line_to(cx, cy + 5.0);
            cr.line_to(cx + 5.0, cy);
            cr.line_to(cx + 17.0, cy + 10.0);
            let _ = cr.stroke();
            cr.arc(cx + 7.0, cy - 7.0, 2.5, 0.0, TAU);
            let _ = cr.stroke();
        }
        Kind::Doc => {
            cr.move_to(cx - 10.0, cy - 15.0);
            cr.line_to(cx + 4.0, cy - 15.0);
            cr.line_to(cx + 10.0, cy - 9.0);
            cr.line_to(cx + 10.0, cy + 15.0);
            cr.line_to(cx - 10.0, cy + 15.0);
            cr.close_path();
            let _ = cr.stroke();
            cr.move_to(cx + 4.0, cy - 15.0);
            cr.line_to(cx + 4.0, cy - 9.0);
            cr.line_to(cx + 10.0, cy - 9.0);
            let _ = cr.stroke();
        }
        Kind::Audio => {
            for (dx, hh) in [(-12.0, 4.0), (-6.0, 8.0), (0.0, 14.0), (6.0, 10.0), (12.0, 6.0)] {
                cr.move_to(cx + dx, cy - hh);
                cr.line_to(cx + dx, cy + hh);
                let _ = cr.stroke();
            }
        }
    }
}

fn draw(cr: &cairo::Context, w: f64, h: f64, st: &State, dark: bool) {
    let p = palette(dark);
    let _ = h;
    cr.translate(((w - W) / 2.0).max(0.0), 0.0);
    let t = st.since.elapsed().as_secs_f64();
    let ease = |x: f64| { let x = x.clamp(0.0, 1.0); 1.0 - (1.0 - x).powi(3) };

    // the window
    cr.set_source_rgb(p.panel.0, p.panel.1, p.panel.2);
    cr.rectangle(0.0, 0.0, W, H);
    let _ = cr.fill();
    cr.set_source_rgb(p.line.0, p.line.1, p.line.2);
    cr.set_line_width(1.0);
    cr.rectangle(0.5, 0.5, W - 1.0, H - 1.0);
    let _ = cr.stroke();
    cr.move_to(0.0, BAR + 0.5);
    cr.line_to(W, BAR + 0.5);
    let _ = cr.stroke();
    text(cr, "Pictures", 12.0, 17.0, 11.0, p.muted, 1.0, true, false);
    text(cr, "-  o  x", W - 52.0, 17.0, 11.0, p.muted, 1.0, false, false);

    // the files, and the copy once it exists
    let count = if st.stage == Stage::Done { TILES.len() + 1 } else { TILES.len() };
    for i in 0..count {
        let r = tile_rect(i);
        let (name, kind, is_new) = if i < TILES.len() {
            (TILES[i].0.to_string(), TILES[i].1, false)
        } else {
            (format!("holiday.{}", SLICES[st.picked.unwrap_or(1)].to_lowercase()), Kind::Image, true)
        };
        let appear = if is_new { ease(t / 0.5) } else { 1.0 };
        cr.save().ok();
        cr.translate(0.0, (1.0 - appear) * 12.0);
        let (mut a, mut lw) = (0.55, 1.0);
        if i == 0 {
            // the file to use: it breathes until it is clicked, then stays marked
            a = if st.stage == Stage::Click { 0.35 + 0.55 * (0.5 + 0.5 * (t * 3.2).sin()) } else { 0.9 };
            lw = 1.6;
        }
        cr.set_source_rgba(if i == 0 { p.fg.0 } else { p.line.0 }, if i == 0 { p.fg.1 } else { p.line.1 }, if i == 0 { p.fg.2 } else { p.line.2 }, a * appear);
        cr.set_line_width(lw);
        cr.rectangle(r.0 + 0.5, r.1 + 0.5, r.2, r.3);
        let _ = cr.stroke();
        icon(cr, kind, r.0 + r.2 / 2.0, r.1 + 32.0, if appear < 1.0 { p.muted } else { p.muted });
        text(cr, &name, r.0 + r.2 / 2.0, r.1 + r.3 - 12.0, 11.0, p.fg, appear, true, true);
        cr.restore().ok();
    }
    let status = match st.stage {
        Stage::Click => "Click holiday.jpg, or right-click it.".to_string(),
        Stage::Menu => "Choose Convert with the format wheel.".to_string(),
        Stage::Wheel => "Pick a format.".to_string(),
        Stage::Done => format!("holiday.{} saved next to the original. holiday.jpg is untouched.", SLICES[st.picked.unwrap_or(1)].to_lowercase()),
    };
    text(cr, &status, 16.0, H - 14.0, 12.0, p.muted, 1.0, false, false);

    // the menu
    if st.stage == Stage::Menu {
        let a = ease(t / 0.18);
        let total: f64 = menu_items().iter().map(|m| m.1).sum::<f64>() + 8.0;
        cr.set_source_rgba(p.panel.0, p.panel.1, p.panel.2, a);
        cr.rectangle(MENU_X, BAR + 4.0, MENU_W, total);
        let _ = cr.fill();
        cr.set_source_rgba(p.line.0, p.line.1, p.line.2, a);
        cr.set_line_width(1.0);
        cr.rectangle(MENU_X + 0.5, BAR + 4.5, MENU_W - 1.0, total - 1.0);
        let _ = cr.stroke();
        for (top, hh, i) in menu_items() {
            if MENU[i] == "-" {
                cr.set_source_rgba(p.line.0, p.line.1, p.line.2, a);
                cr.move_to(MENU_X + 8.0, top + hh / 2.0);
                cr.line_to(MENU_X + MENU_W - 8.0, top + hh / 2.0);
                let _ = cr.stroke();
                continue;
            }
            let hot = st.hover == Some(i);
            if hot {
                cr.set_source_rgba(p.fg.0, p.fg.1, p.fg.2, a);
                cr.rectangle(MENU_X + 1.0, top, MENU_W - 2.0, hh);
                let _ = cr.fill();
            }
            text(cr, MENU[i], MENU_X + 12.0, top + 15.0, 11.5, if hot { p.bg } else { p.fg }, a, i == WHEEL_ITEM, false);
            if MENU[i] == "Convert to" {
                text(cr, ">", MENU_X + MENU_W - 18.0, top + 15.0, 11.5, if hot { p.bg } else { p.muted }, a, false, false);
            }
        }
    }

    // the wheel, kept inside the window
    if st.stage == Stage::Wheel {
        let k = ease(t / 0.4);
        let (cx, cy) = ring_center();
        cr.save().ok();
        cr.translate(cx, cy);
        cr.scale(0.82 + 0.18 * k, 0.82 + 0.18 * k);
        // a soft shadow under the ring, so it lifts off the files behind it
        for (grow, a) in [(14.0, 0.10), (8.0, 0.14), (3.0, 0.18)] {
            cr.new_path();
            cr.set_source_rgba(0.0, 0.0, 0.0, a * k);
            cr.arc(0.0, 5.0, RING_R + grow, 0.0, TAU);
            let _ = cr.fill();
        }
        for (i, name) in SLICES.iter().enumerate() {
            let a0 = (-150.0 + 120.0 * i as f64 + 3.0) * PI / 180.0;
            let a1 = (-150.0 + 120.0 * (i + 1) as f64 - 3.0) * PI / 180.0;
            let hot = st.hover == Some(i);
            cr.new_path();
            cr.arc(0.0, 0.0, RING_R, a0, a1);
            cr.arc_negative(0.0, 0.0, HUB_R + 4.0, a1, a0);
            cr.close_path();
            if hot {
                cr.set_source_rgba(p.fg.0, p.fg.1, p.fg.2, k);
            } else {
                cr.set_source_rgba(p.lift.0, p.lift.1, p.lift.2, k);
            }
            let _ = cr.fill_preserve();
            cr.set_source_rgba(p.fg.0, p.fg.1, p.fg.2, 0.5 * k);
            cr.set_line_width(1.5);
            let _ = cr.stroke();
            let m = (a0 + a1) / 2.0;
            let r = (RING_R + HUB_R) / 2.0 + 6.0;
            text(cr, name, m.cos() * r, m.sin() * r + 4.0, 13.0, if hot { p.bg } else { p.fg }, k, true, true);
        }
        cr.new_path();
        cr.set_source_rgba(p.bg.0, p.bg.1, p.bg.2, k);
        cr.arc(0.0, 0.0, HUB_R, 0.0, TAU);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(p.line.0, p.line.1, p.line.2, k);
        let _ = cr.stroke();
        text(cr, "Pick one", 0.0, -2.0, 10.5, p.muted, k, true, true);
        text(cr, "holiday.jpg", 0.0, 12.0, 9.5, p.muted, k, false, true);
        cr.restore().ok();
    }
}

/// Builds the practice window. `on_stage` is told 1 to 4 whenever the tutorial moves on,
/// so the step list beside it can follow. The returned closure puts it back at the start.
pub fn build(dark: Rc<Cell<bool>>, on_stage: Rc<dyn Fn(Stage)>) -> (gtk4::DrawingArea, Rc<dyn Fn()>) {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(W as i32);
    area.set_content_height(H as i32);
    area.set_halign(gtk4::Align::Start);
    area.set_focusable(true);
    area.update_property(&[gtk4::accessible::Property::Label("Practice window for the format wheel")]);
    let st = Rc::new(RefCell::new(State { stage: Stage::Click, since: Instant::now(), hover: None, picked: None }));

    let offset = {
        let area = area.clone();
        move || ((area.width() as f64 - W) / 2.0).max(0.0)
    };
    let set_stage = {
        let (st, area, on_stage) = (st.clone(), area.clone(), on_stage.clone());
        Rc::new(move |s: Stage, picked: Option<usize>| {
            {
                let mut b = st.borrow_mut();
                b.stage = s;
                b.since = Instant::now();
                b.hover = None;
                if picked.is_some() { b.picked = picked; }
            }
            on_stage(s);
            area.queue_draw();
        })
    };

    {
        let (st, dark) = (st.clone(), dark.clone());
        area.set_draw_func(move |_, cr, w, h| draw(cr, w as f64, h as f64, &st.borrow(), dark.get()));
    }
    // redraw while something moves: the breathing outline, or an entrance
    {
        let st = st.clone();
        area.add_tick_callback(move |w, _| {
            if w.is_mapped() {
                let b = st.borrow();
                if b.stage == Stage::Click || b.since.elapsed().as_millis() < 700 {
                    w.queue_draw();
                }
            }
            glib::ControlFlow::Continue
        });
    }
    // a click moves the practice along
    {
        let (st, set_stage, offset) = (st.clone(), set_stage.clone(), offset.clone());
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.connect_pressed(move |g, _, x, y| {
            if let Some(a) = g.widget() { a.grab_focus(); }
            let (x, y) = (x - offset(), y);
            let stage = st.borrow().stage;
            match stage {
                Stage::Click if in_rect(x, y, tile_rect(0)) => set_stage(Stage::Menu, None),
                Stage::Menu => {
                    let hit = menu_items().into_iter().find(|(top, hh, _)| x >= MENU_X && x <= MENU_X + MENU_W && y >= *top && y <= top + hh);
                    match hit {
                        Some((_, _, i)) if i == WHEEL_ITEM => set_stage(Stage::Wheel, None),
                        Some(_) => {}
                        None => set_stage(Stage::Click, None),
                    }
                }
                Stage::Wheel => {
                    if let Some(i) = slice_at(x, y) { set_stage(Stage::Done, Some(i)) }
                }
                _ => {}
            }
        });
        area.add_controller(click);
    }
    // hover highlights the item or slice under the pointer
    {
        let (st, area2, offset) = (st.clone(), area.clone(), offset.clone());
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion(move |_, x, y| {
            let (x, y) = (x - offset(), y);
            let mut b = st.borrow_mut();
            let hover = match b.stage {
                Stage::Menu => menu_items().into_iter().find(|(top, hh, i)| MENU[*i] != "-" && x >= MENU_X && x <= MENU_X + MENU_W && y >= *top && y <= top + hh).map(|m| m.2),
                Stage::Wheel => slice_at(x, y),
                _ => None,
            };
            if hover != b.hover {
                b.hover = hover;
                area2.queue_draw();
            }
        });
        area.add_controller(motion);
    }
    // keyboard: Enter or Space does the next move
    {
        let (st, set_stage) = (st.clone(), set_stage.clone());
        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, _| {
            use gtk4::gdk::Key;
            if !matches!(key, Key::Return | Key::KP_Enter | Key::space) {
                return glib::Propagation::Proceed;
            }
            let stage = st.borrow().stage;
            match stage {
                Stage::Click => set_stage(Stage::Menu, None),
                Stage::Menu => set_stage(Stage::Wheel, None),
                Stage::Wheel => set_stage(Stage::Done, Some(1)),
                Stage::Done => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        area.add_controller(keys);
    }
    // ARKA_OOBE_TUT=2|3|4 (preview only) opens the practice window at that move, for design review.
    let start = if std::env::var("ARKA_OOBE_PREVIEW").is_ok() {
        match std::env::var("ARKA_OOBE_TUT").as_deref() {
            Ok("2") => Stage::Menu,
            Ok("3") => Stage::Wheel,
            Ok("4") => Stage::Done,
            _ => Stage::Click,
        }
    } else {
        Stage::Click
    };
    let reset: Rc<dyn Fn()> = {
        let set_stage = set_stage.clone();
        Rc::new(move || set_stage(start, if start == Stage::Done { Some(1) } else { None }))
    };
    (area, reset)
}
