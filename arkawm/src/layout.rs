//! Window placement. By default windows overlap as on Windows or macOS and
//! snap to screen halves, quarters or full screen. The scrolling row is an
//! option: windows side by side at full height, scrolling sideways.

use smithay::{
    desktop::{layer_map_for_output, Window},
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle, Size, SERIAL_COUNTER},
};

use crate::titlebar::BAR;
use crate::state::{ArkaWm, LayoutMode, Managed, Snap, WindowState};

/// Space between windows and around the screen edge.
pub const GAP: i32 = 8;
/// Widths a window cycles through in the row, as a share of the screen.
pub const ROW_WIDTHS: [f64; 3] = [1.0 / 3.0, 0.5, 2.0 / 3.0];
/// Width of a newly opened window in the row.
pub const DEFAULT_ROW_WIDTH: f64 = 0.5;

/// The new (state, restore) when full screen is asked on or off, or None if nothing changes.
/// Going in remembers what the window was; coming out puts that back.
pub fn fullscreen_change(state: WindowState, restore: WindowState, on: bool) -> Option<(WindowState, WindowState)> {
    match (on, state == WindowState::Fullscreen) {
        (true, false) => Some((WindowState::Fullscreen, state)),
        (false, true) => Some((restore, restore)),
        _ => None,
    }
}

impl ArkaWm {
    /// The output's full area, in global coordinates.
    pub fn output_area(&self) -> Rectangle<i32, Logical> {
        self.space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
            .unwrap_or_default()
    }

    /// The part of the screen not reserved by panels and docks.
    pub fn usable_area(&self) -> Rectangle<i32, Logical> {
        let Some(output) = self.space.outputs().next() else {
            return Rectangle::default();
        };
        let mut zone = layer_map_for_output(output).non_exclusive_zone();
        zone.loc += self.output_area().loc;
        zone
    }

    pub fn add_window(&mut self, window: Window) {
        tracing::info!("new window");
        let area = self.usable_area();
        // Until the window's first frame says how big it is, cascade it.
        let offset = 32 * (self.windows.len() as i32 % 8);
        let rect = Rectangle::new(
            area.loc + Point::from((area.size.w / 6 + offset, area.size.h / 8 + offset)),
            Size::from((area.size.w * 2 / 3, area.size.h * 3 / 4)),
        );
        let at = if self.windows.is_empty() { 0 } else { self.focused + 1 };
        let id = self.next_window_id;
        self.next_window_id += 1;
        self.windows.insert(
            at,
            Managed {
                window,
                id,
                minimized: false,
                rect,
                state: WindowState::Normal,
                restore: WindowState::Normal,
                placed: false,
                row_width: DEFAULT_ROW_WIDTH,
                decorated: false,
                bar: None,
            },
        );
        self.focus_index(at);
    }

    /// Centre a new window in the free area once its size is known.
    pub fn place_new_window(&mut self, surface: &WlSurface) {
        let area = self.usable_area();
        let Some(index) = self.index_of(surface) else { return };
        let managed = &mut self.windows[index];
        if managed.placed {
            return;
        }
        let size = managed.window.geometry().size;
        if size.w <= 0 || size.h <= 0 {
            return;
        }
        managed.placed = true;
        // `rect` is the whole frame: the app's window plus the title bar, if ArkaWM draws one.
        let bar = if managed.decorated { BAR } else { 0 };
        let size = Size::from((size.w.min(area.size.w), (size.h + bar).min(area.size.h)));
        let mut loc = area.loc + Point::from(((area.size.w - size.w) / 2, (area.size.h - size.h) / 2));
        // Step down and right while another window already sits there, so a
        // new window never hides one exactly.
        const STEP: i32 = 32;
        let taken: Vec<Point<i32, Logical>> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(i, m)| *i != index && m.placed && m.state == WindowState::Normal)
            .map(|(_, m)| m.rect.loc)
            .collect();
        for _ in 0..12 {
            if !taken.contains(&loc) {
                break;
            }
            loc += Point::from((STEP, STEP));
        }
        // Keep the title bar on screen.
        loc.x = loc.x.min(area.loc.x + area.size.w - size.w).max(area.loc.x);
        loc.y = loc.y.min(area.loc.y + area.size.h - size.h).max(area.loc.y);
        self.windows[index].rect = Rectangle::new(loc, size);
        self.arrange();
    }

    pub fn remove_window(&mut self, surface: &WlSurface) {
        let Some(index) = self.index_of(surface) else { return };
        let managed = self.windows.remove(index);
        self.space.unmap_elem(&managed.window);
        if self.windows.is_empty() {
            self.focused = 0;
            let serial = SERIAL_COUNTER.next_serial();
            self.focus_surface(None, serial);
            return;
        }
        self.focused = self.focused.min(self.windows.len() - 1);
        // Focus the window that was on top before, as on other desktops.
        self.focus_top();
    }

    /// Give the focus to the window on top, or to nothing if every window is put away.
    pub fn focus_top(&mut self) {
        let top = self.space.elements().last().and_then(|w| self.index_of_window(w));
        match top {
            Some(index) => self.focus_index(index),
            None => {
                // nothing on screen: the keyboard goes to the desktop (its apps), or nowhere
                let serial = SERIAL_COUNTER.next_serial();
                let desktop = self.desktop_surface();
                self.focus_surface(desktop, serial);
                self.redraw_wanted = true;
            }
        }
    }

    /// Put a window away (minimise) or bring it back in front.
    pub fn set_minimized(&mut self, index: usize, on: bool) {
        if self.mode != LayoutMode::Free || index >= self.windows.len() {
            return;
        }
        if !on {
            self.focus_index(index);
            return;
        }
        let managed = &mut self.windows[index];
        if managed.minimized {
            return;
        }
        managed.minimized = true;
        managed.window.set_activated(false);
        if let Some(toplevel) = managed.window.toplevel() {
            toplevel.send_pending_configure();
        }
        let window = managed.window.clone();
        self.space.unmap_elem(&window);
        self.focus_top();
    }

    /// Show the desktop: put every window away; again brings the same windows back.
    pub fn toggle_show_desktop(&mut self) {
        if self.mode != LayoutMode::Free {
            return;
        }
        let showing: Vec<usize> = (0..self.windows.len()).filter(|&i| !self.windows[i].minimized).collect();
        if showing.is_empty() {
            for id in std::mem::take(&mut self.desktop_hidden) {
                if let Some(index) = self.windows.iter().position(|m| m.id == id) {
                    self.focus_index(index);
                }
            }
            return;
        }
        self.desktop_hidden =
            self.space.elements().filter_map(|w| self.index_of_window(w)).map(|i| self.windows[i].id).collect();
        for index in showing {
            self.set_minimized(index, true);
        }
    }

    pub fn index_of(&self, surface: &WlSurface) -> Option<usize> {
        self.windows
            .iter()
            .position(|m| m.window.toplevel().map(|t| t.wl_surface() == surface).unwrap_or(false))
    }

    pub fn index_of_window(&self, window: &Window) -> Option<usize> {
        self.windows.iter().position(|m| &m.window == window)
    }

    /// Focus the window at `index`, raise it and give it the keyboard.
    /// Give the keyboard to a window. While the screen is locked the keyboard stays with the
    /// lock screen, whatever windows open, close or ask for.
    pub fn focus_surface(&mut self, surface: Option<WlSurface>, serial: smithay::utils::Serial) {
        if self.locked {
            return;
        }
        self.seat.get_keyboard().unwrap().set_focus(self, surface, serial);
    }

    pub fn focus_index(&mut self, index: usize) {
        if index >= self.windows.len() {
            return;
        }
        self.focused = index;
        self.windows[index].minimized = false;
        for (i, managed) in self.windows.iter().enumerate() {
            managed.window.set_activated(i == index);
        }
        let window = self.windows[index].window.clone();
        self.space.raise_element(&window, true);
        let surface = window.toplevel().unwrap().wl_surface().clone();
        let serial = SERIAL_COUNTER.next_serial();
        self.focus_surface(Some(surface), serial);
        self.arrange();
    }

    /// Next or previous window, in the order they were opened.
    pub fn focus_step(&mut self, step: isize) {
        if self.windows.is_empty() {
            return;
        }
        let n = self.windows.len() as isize;
        let target = match self.mode {
            LayoutMode::Row => (self.focused as isize + step).clamp(0, n - 1),
            LayoutMode::Free => (self.focused as isize + step).rem_euclid(n),
        };
        self.focus_index(target as usize);
    }

    /// In the row: swap the focused window with its neighbour.
    pub fn move_focused(&mut self, step: isize) {
        let target = self.focused as isize + step;
        if target < 0 || target >= self.windows.len() as isize {
            return;
        }
        self.windows.swap(self.focused, target as usize);
        self.focus_index(target as usize);
    }

    pub fn set_state(&mut self, index: usize, state: WindowState) {
        if let Some(managed) = self.windows.get_mut(index) {
            managed.state = state;
            self.arrange();
        }
    }

    /// Alt+Left / Alt+Right: snap to a half, or step through the row.
    pub fn snap_focused(&mut self, snap: Snap) {
        if self.mode == LayoutMode::Row {
            self.focus_step(if snap == Snap::Left { -1 } else { 1 });
            return;
        }
        let Some(managed) = self.windows.get(self.focused) else { return };
        // Snapping to the side it is already on puts it back.
        let state = if managed.state == WindowState::Snapped(snap) {
            WindowState::Normal
        } else {
            WindowState::Snapped(snap)
        };
        self.set_state(self.focused, state);
    }

    /// Full screen on or off for one window. Leaving it goes back to what the window was before.
    pub fn set_fullscreen(&mut self, index: usize, on: bool) {
        let Some(managed) = self.windows.get_mut(index) else { return };
        let Some((state, restore)) = fullscreen_change(managed.state, managed.restore, on) else { return };
        managed.state = state;
        managed.restore = restore;
        self.arrange();
    }

    pub fn toggle_fullscreen_focused(&mut self) {
        let Some(managed) = self.windows.get(self.focused) else { return };
        let on = managed.state != WindowState::Fullscreen;
        self.set_fullscreen(self.focused, on);
    }

    /// The focused window, if it is full screen in the free layout. Only this one is drawn
    /// above the panels: when the focus moves to another window, the panels come back.
    pub fn fullscreen_window(&self) -> Option<Window> {
        if self.mode != LayoutMode::Free {
            return None;
        }
        let managed = self.windows.get(self.focused)?;
        (managed.state == WindowState::Fullscreen && !managed.minimized).then(|| managed.window.clone())
    }

    pub fn toggle_maximize_focused(&mut self) {
        let Some(managed) = self.windows.get(self.focused) else { return };
        let state = if managed.state == WindowState::Maximized {
            WindowState::Normal
        } else {
            WindowState::Maximized
        };
        self.set_state(self.focused, state);
    }

    /// In the row: the next preset width.
    pub fn cycle_width(&mut self) {
        if self.mode == LayoutMode::Free {
            return;
        }
        let Some(managed) = self.windows.get_mut(self.focused) else { return };
        managed.row_width = ROW_WIDTHS
            .iter()
            .copied()
            .find(|w| *w > managed.row_width + 0.01)
            .unwrap_or(ROW_WIDTHS[0]);
        self.arrange();
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            LayoutMode::Free => LayoutMode::Row,
            LayoutMode::Row => LayoutMode::Free,
        };
        self.view_x = 0;
        // the row has no minimised windows
        self.windows.iter_mut().for_each(|m| m.minimized = false);
        self.arrange();
    }

    pub fn close_focused(&mut self) {
        if let Some(managed) = self.windows.get(self.focused) {
            managed.window.toplevel().unwrap().send_close();
        }
    }

    /// The rectangle a snapped window takes inside `area`.
    pub fn snap_rect(snap: Snap, area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        let half_w = (area.size.w - 3 * GAP) / 2;
        let half_h = (area.size.h - 3 * GAP) / 2;
        let full_h = area.size.h - 2 * GAP;
        let left = area.loc.x + GAP;
        let right = area.loc.x + 2 * GAP + half_w;
        let top = area.loc.y + GAP;
        let bottom = area.loc.y + 2 * GAP + half_h;
        let (x, y, w, h) = match snap {
            Snap::Left => (left, top, half_w, full_h),
            Snap::Right => (right, top, half_w, full_h),
            Snap::TopLeft => (left, top, half_w, half_h),
            Snap::TopRight => (right, top, half_w, half_h),
            Snap::BottomLeft => (left, bottom, half_w, half_h),
            Snap::BottomRight => (right, bottom, half_w, half_h),
        };
        Rectangle::new((x, y).into(), (w, h).into())
    }

    /// Where a window goes in its current state.
    pub fn target_rect(&self, managed: &Managed) -> Rectangle<i32, Logical> {
        let area = self.usable_area();
        match managed.state {
            WindowState::Normal => managed.rect,
            WindowState::Maximized => area,
            WindowState::Fullscreen => self.output_area(),
            WindowState::Snapped(snap) => Self::snap_rect(snap, area),
        }
    }

    /// Height of the title bar ArkaWM draws for this window right now (none when full screen).
    pub fn bar_height(&self, managed: &Managed) -> i32 {
        if managed.decorated && managed.state != WindowState::Fullscreen {
            BAR
        } else {
            0
        }
    }

    pub fn bar_height_of(&self, window: &Window) -> i32 {
        self.index_of_window(window).map(|i| self.bar_height(&self.windows[i])).unwrap_or(0)
    }

    /// Where a window's title bar is on screen, if ArkaWM draws one.
    pub fn bar_rect(&self, managed: &Managed) -> Option<Rectangle<i32, Logical>> {
        let bar = self.bar_height(managed);
        if bar == 0 {
            return None;
        }
        let loc = self.space.element_location(&managed.window)?;
        let width = managed.window.geometry().size.w;
        (width > 0).then(|| Rectangle::new((loc.x, loc.y - bar).into(), (width, bar).into()))
    }

    /// The title bar under a point, unless a window above it covers that point:
    /// the window's index and the button the point is on, if any.
    pub fn bar_at(&self, pos: Point<f64, Logical>) -> Option<(usize, Option<crate::titlebar::Button>)> {
        for window in self.space.elements().rev() {
            let index = self.index_of_window(window)?;
            if let Some(bar) = self.bar_rect(&self.windows[index]) {
                if bar.to_f64().contains(pos) {
                    return Some((index, crate::titlebar::button_at(bar, pos)));
                }
            }
            let loc = self.space.element_location(window)?;
            if Rectangle::new(loc, window.geometry().size).to_f64().contains(pos) {
                return None;
            }
        }
        None
    }

    /// Redraw the title bars whose title, width or focus changed.
    pub fn update_bars(&mut self) {
        let scale120 = (self.scale * 120.0).round() as i32;
        for index in 0..self.windows.len() {
            let managed = &self.windows[index];
            if self.bar_height(managed) == 0 {
                self.windows[index].bar = None;
                continue;
            }
            let title = smithay::wayland::compositor::with_states(managed.window.toplevel().unwrap().wl_surface(), |states| {
                states
                    .data_map
                    .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                    .and_then(|data| data.lock().unwrap().title.clone())
                    .unwrap_or_default()
            });
            let key = crate::titlebar::BarKey {
                width: managed.window.geometry().size.w,
                title,
                focused: index == self.focused,
                scale120,
                hover: self.bar_hover.filter(|(at, _)| *at == index).map(|(_, button)| button),
            };
            if key.width > 0 && managed.bar.as_ref().map(|(k, _)| k) != Some(&key) {
                let buffer = crate::titlebar::draw(&key);
                self.windows[index].bar = Some((key, buffer));
            }
        }
    }

    /// Place every window and tell each its size.
    pub fn arrange(&mut self) {
        let area = self.usable_area();
        if area.size.w <= 0 || area.size.h <= 0 {
            return;
        }
        match self.mode {
            LayoutMode::Free => self.arrange_free(),
            LayoutMode::Row => self.arrange_row(area),
        }
    }

    fn arrange_free(&mut self) {
        let bounds = self.usable_area().size;
        let placements: Vec<(Window, Rectangle<i32, Logical>, WindowState, bool)> = self
            .windows
            .iter()
            .filter(|m| !m.minimized)
            .map(|m| (m.window.clone(), below_bar(self.target_rect(m), self.bar_height(m)), m.state, m.placed))
            .collect();
        // Placing a window puts it on top, so go from the bottom of the stack to the top: the
        // order of the windows on screen stays as it was, and a new window lands on top.
        let stack: Vec<Window> = self.space.elements().cloned().collect();
        let mut placements = placements;
        placements.sort_by_key(|(window, ..)| stack.iter().position(|w| w == window).unwrap_or(usize::MAX));
        for (window, rect, state, placed) in placements {
            let toplevel = window.toplevel().unwrap();
            toplevel.with_pending_state(|s| {
                let tiled = matches!(state, WindowState::Snapped(_));
                set_flag(&mut s.states, xdg_toplevel::State::Maximized, state == WindowState::Maximized);
                set_flag(&mut s.states, xdg_toplevel::State::Fullscreen, state == WindowState::Fullscreen);
                for flag in [
                    xdg_toplevel::State::TiledLeft,
                    xdg_toplevel::State::TiledRight,
                    xdg_toplevel::State::TiledTop,
                    xdg_toplevel::State::TiledBottom,
                ] {
                    set_flag(&mut s.states, flag, tiled);
                }
                // Tell apps the largest size that fits, so new windows open on screen.
                s.bounds = Some(bounds);
                // A normal window keeps the size it chose until it is placed.
                s.size = if state == WindowState::Normal && !placed {
                    None
                } else {
                    Some(rect.size)
                };
            });
            if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            }
            self.space.map_element(window.clone(), rect.loc, false);
        }
    }

    fn arrange_row(&mut self, area: Rectangle<i32, Logical>) {
        let inner_w = area.size.w - 2 * GAP;
        let height = area.size.h - 2 * GAP;

        let mut xs = Vec::with_capacity(self.windows.len());
        let mut x = 0;
        for managed in &self.windows {
            let w = row_width(managed.row_width, inner_w);
            xs.push((x, w));
            x += w + GAP;
        }
        if let Some(&(fx, fw)) = xs.get(self.focused) {
            if fx < self.view_x {
                self.view_x = fx;
            } else if fx + fw > self.view_x + inner_w {
                self.view_x = fx + fw - inner_w;
            }
        }
        // Don't leave empty space at the right end when the row fits.
        let total = x - GAP;
        self.view_x = self.view_x.min((total - inner_w).max(0)).max(0);

        for (managed, &(x, w)) in self.windows.iter().zip(&xs) {
            let toplevel = managed.window.toplevel().unwrap();
            toplevel.with_pending_state(|s| {
                s.size = Some((w, height - self.bar_height(managed)).into());
                set_flag(&mut s.states, xdg_toplevel::State::Maximized, false);
                for flag in [
                    xdg_toplevel::State::TiledLeft,
                    xdg_toplevel::State::TiledRight,
                    xdg_toplevel::State::TiledTop,
                    xdg_toplevel::State::TiledBottom,
                ] {
                    set_flag(&mut s.states, flag, true);
                }
            });
            if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            }
            let loc = (area.loc.x + GAP + x - self.view_x, area.loc.y + GAP + self.bar_height(managed));
            self.space.map_element(managed.window.clone(), loc, false);
        }
    }
}

/// The part of a frame below its title bar: where the app's own window goes.
pub fn below_bar(frame: Rectangle<i32, Logical>, bar: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((frame.loc.x, frame.loc.y + bar).into(), (frame.size.w, (frame.size.h - bar).max(1)).into())
}

fn set_flag(
    states: &mut smithay::wayland::shell::xdg::ToplevelStateSet,
    flag: xdg_toplevel::State,
    on: bool,
) {
    if on {
        states.set(flag);
    } else {
        states.unset(flag);
    }
}

fn row_width(share: f64, inner_w: i32) -> i32 {
    ((inner_w + GAP) as f64 * share) as i32 - GAP
}

#[cfg(test)]
mod fullscreen_tests {
    use super::*;
    use crate::state::Snap;

    #[test]
    fn the_app_window_sits_below_its_title_bar() {
        let frame = Rectangle::new((10, 20).into(), (300, 200).into());
        assert_eq!(below_bar(frame, 32), Rectangle::new((10, 52).into(), (300, 168).into()));
        assert_eq!(below_bar(frame, 0), frame);
    }

    #[test]
    fn full_screen_remembers_and_restores_what_the_window_was() {
        let snapped = WindowState::Snapped(Snap::Left);
        let (state, restore) = fullscreen_change(snapped, WindowState::Normal, true).unwrap();
        assert_eq!((state, restore), (WindowState::Fullscreen, snapped));
        assert_eq!(fullscreen_change(state, restore, false), Some((snapped, snapped)));
        // asking for what it already is changes nothing
        assert_eq!(fullscreen_change(WindowState::Fullscreen, snapped, true), None);
        assert_eq!(fullscreen_change(WindowState::Maximized, WindowState::Normal, false), None);
    }
}
