//! How the setup changes page: the old page fades away, the ridge marker travels (see
//! ridge.rs), then the new page builds in piece by piece, title first, each part 70 ms after
//! the one before. About a second from click to settled, slowed on purpose: a quick slide
//! made the setup feel rushed on real hardware.
//!
//! Everything here is opacity and a small margin change, which the CPU-only renderer can
//! handle. When the desktop asks for no animations, none of it runs.

use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

const EXIT_MS: u32 = 240;
const ENTER_MS: u32 = 560;
const STAGGER_MS: u64 = 70;
const RISE_PX: i32 = 22;

pub fn animations_enabled() -> bool {
    gtk4::Settings::default().map(|s| s.is_gtk_enable_animations()).unwrap_or(true)
}

/// Fade `w` out, then call `done`. `w` is back at full opacity afterwards, ready for next time.
pub fn exit(w: &gtk4::Widget, done: impl Fn() + 'static) {
    let target = {
        let w = w.clone();
        adw::CallbackAnimationTarget::new(move |v| w.set_opacity(v))
    };
    let a = adw::TimedAnimation::new(w, 1.0, 0.0, EXIT_MS, target);
    a.set_easing(adw::Easing::EaseInCubic);
    let w2 = w.clone();
    a.connect_done(move |_| {
        done();
        w2.set_opacity(1.0);
    });
    a.play();
}

fn find_column(w: &gtk4::Widget) -> Option<gtk4::Widget> {
    if w.has_css_class("oobe-page") && w.is::<gtk4::Box>() {
        return Some(w.clone());
    }
    let mut c = w.first_child();
    while let Some(child) = c {
        if let Some(f) = find_column(&child) {
            return Some(f);
        }
        c = child.next_sibling();
    }
    None
}

/// Hide the parts of `page` so they can build in. Call before the page is shown.
pub fn prepare(page: &gtk4::Widget) -> Vec<(gtk4::Widget, i32)> {
    let mut parts = Vec::new();
    if let Some(col) = find_column(page) {
        let mut c = col.first_child();
        while let Some(child) = c {
            let base = child.margin_top();
            child.set_opacity(0.0);
            child.set_margin_top(base + RISE_PX);
            parts.push((child.clone(), base));
            c = child.next_sibling();
        }
    }
    parts
}

/// Build the prepared parts in, one after another.
pub fn play(parts: Vec<(gtk4::Widget, i32)>) {
    for (k, (w, base)) in parts.into_iter().enumerate() {
        let start = move || {
            let target = {
                let w = w.clone();
                adw::CallbackAnimationTarget::new(move |v| {
                    w.set_opacity(v);
                    w.set_margin_top(base + ((1.0 - v) * RISE_PX as f64).round() as i32);
                })
            };
            let a = adw::TimedAnimation::new(&w, 0.0, 1.0, ENTER_MS, target);
            a.set_easing(adw::Easing::EaseOutCubic);
            a.play();
        };
        if k == 0 {
            start();
        } else {
            glib::timeout_add_local_once(Duration::from_millis(STAGGER_MS * k as u64), start);
        }
    }
}
