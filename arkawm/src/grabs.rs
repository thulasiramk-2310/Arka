//! Moving and resizing windows with the pointer. Dragging a window to the
//! left or right screen edge snaps it to that half, to a corner snaps it to a
//! quarter, and to the top edge maximises it, as on Windows 11.

use smithay::{
    desktop::Window,
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab,
        PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel, wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{Logical, Point, Rectangle, Size},
};

use crate::state::{ArkaWm, LayoutMode, Snap, WindowState};

/// How close to a screen edge the pointer must be to snap, in logical pixels.
const SNAP_EDGE: f64 = 8.0;
/// Smallest size a window can be resized to.
const MIN_SIZE: (i32, i32) = (240, 160);

/// The snap a pointer position on screen `out` asks for, if any.
pub fn snap_at(out: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<WindowState> {
    let out = out.to_f64();
    let (left, top) = (out.loc.x, out.loc.y);
    let (right, bottom) = (left + out.size.w, top + out.size.h);
    let upper = top + out.size.h / 4.0;
    let lower = bottom - out.size.h / 4.0;
    if pos.x <= left + SNAP_EDGE {
        Some(WindowState::Snapped(if pos.y < upper {
            Snap::TopLeft
        } else if pos.y > lower {
            Snap::BottomLeft
        } else {
            Snap::Left
        }))
    } else if pos.x >= right - SNAP_EDGE - 1.0 {
        Some(WindowState::Snapped(if pos.y < upper {
            Snap::TopRight
        } else if pos.y > lower {
            Snap::BottomRight
        } else {
            Snap::Right
        }))
    } else if pos.y <= top + SNAP_EDGE {
        Some(WindowState::Maximized)
    } else {
        None
    }
}

pub struct MoveGrab {
    pub start_data: PointerGrabStartData<ArkaWm>,
    pub window: Window,
    pub initial_location: Point<i32, Logical>,
    pub snap: Option<WindowState>,
}

impl MoveGrab {
    /// Start moving `window`. A maximised or snapped window first returns to
    /// its normal size under the pointer.
    pub fn start(state: &mut ArkaWm, start_data: PointerGrabStartData<ArkaWm>, window: Window) -> Option<Self> {
        if state.mode == LayoutMode::Row {
            return None;
        }
        let index = state.index_of_window(&window)?;
        let pointer = start_data.location;
        // Positions here are of the whole frame: the title bar, if any, sits above the app's window.
        let bar = state.bar_height_of(&window);
        let current = state.space.element_location(&window)? - Point::from((0, bar));
        let managed = &mut state.windows[index];
        let mut initial_location = current;
        if managed.state != WindowState::Normal {
            let geo = managed.window.geometry();
            let fraction = if geo.size.w > 0 {
                (pointer.x - current.x as f64) / geo.size.w as f64
            } else {
                0.5
            };
            initial_location = Point::from((
                (pointer.x - fraction * managed.rect.size.w as f64) as i32,
                current.y,
            ));
            managed.rect.loc = initial_location;
            managed.state = WindowState::Normal;
            state.arrange();
        }
        Some(Self { start_data, window, initial_location, snap: None })
    }
}

impl PointerGrab<ArkaWm> for MoveGrab {
    fn motion(
        &mut self,
        data: &mut ArkaWm,
        handle: &mut PointerInnerHandle<'_, ArkaWm>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus.
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;
        let new_location = (self.initial_location.to_f64() + delta).to_i32_round();
        if let Some(index) = data.index_of_window(&self.window) {
            data.windows[index].rect.loc = new_location;
            let bar = data.bar_height_of(&self.window);
            data.space.map_element(self.window.clone(), new_location + Point::from((0, bar)), true);
        }

        self.snap = snap_at(data.output_area(), event.location);
        data.snap_preview = self.snap.map(|snap| {
            let area = data.usable_area();
            match snap {
                WindowState::Snapped(s) => ArkaWm::snap_rect(s, area),
                _ => area,
            }
        });
    }

    fn relative_motion(
        &mut self,
        data: &mut ArkaWm,
        handle: &mut PointerInnerHandle<'_, ArkaWm>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &ButtonEvent) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, details: AxisFrame) {
        handle.axis(data, details)
    }

    fn frame(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureSwipeBeginEvent) {
        handle.gesture_swipe_begin(data, event)
    }
    fn gesture_swipe_update(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureSwipeUpdateEvent) {
        handle.gesture_swipe_update(data, event)
    }
    fn gesture_swipe_end(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureSwipeEndEvent) {
        handle.gesture_swipe_end(data, event)
    }
    fn gesture_pinch_begin(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GesturePinchBeginEvent) {
        handle.gesture_pinch_begin(data, event)
    }
    fn gesture_pinch_update(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GesturePinchUpdateEvent) {
        handle.gesture_pinch_update(data, event)
    }
    fn gesture_pinch_end(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GesturePinchEndEvent) {
        handle.gesture_pinch_end(data, event)
    }
    fn gesture_hold_begin(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureHoldBeginEvent) {
        handle.gesture_hold_begin(data, event)
    }
    fn gesture_hold_end(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureHoldEndEvent) {
        handle.gesture_hold_end(data, event)
    }

    fn start_data(&self) -> &PointerGrabStartData<ArkaWm> {
        &self.start_data
    }

    /// Released: snap if the pointer was in a snap zone.
    fn unset(&mut self, data: &mut ArkaWm) {
        data.snap_preview = None;
        if let (Some(snap), Some(index)) = (self.snap, data.index_of_window(&self.window)) {
            data.set_state(index, snap);
        }
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Edges: u32 {
        const TOP = 0b0001;
        const BOTTOM = 0b0010;
        const LEFT = 0b0100;
        const RIGHT = 0b1000;
    }
}

impl From<xdg_toplevel::ResizeEdge> for Edges {
    fn from(edge: xdg_toplevel::ResizeEdge) -> Self {
        Self::from_bits(edge as u32).unwrap_or(Edges::empty())
    }
}

pub struct ResizeGrab {
    pub start_data: PointerGrabStartData<ArkaWm>,
    pub window: Window,
    pub edges: Edges,
    pub initial: Rectangle<i32, Logical>,
}

impl ResizeGrab {
    pub fn start(
        state: &mut ArkaWm,
        start_data: PointerGrabStartData<ArkaWm>,
        window: Window,
        edges: Edges,
    ) -> Option<Self> {
        if state.mode == LayoutMode::Row {
            return None;
        }
        let index = state.index_of_window(&window)?;
        // Resizing a snapped or maximised window starts from where it is now.
        let bar = state.bar_height_of(&window);
        let loc = state.space.element_location(&window)? - Point::from((0, bar));
        let initial = Rectangle::new(loc, window.geometry().size + Size::from((0, bar)));
        let managed = &mut state.windows[index];
        managed.rect = initial;
        managed.state = WindowState::Normal;
        managed.window.toplevel().unwrap().with_pending_state(|s| {
            s.states.set(xdg_toplevel::State::Resizing);
        });
        state.arrange();
        Some(Self { start_data, window, edges, initial })
    }
}

impl PointerGrab<ArkaWm> for ResizeGrab {
    fn motion(
        &mut self,
        data: &mut ArkaWm,
        handle: &mut PointerInnerHandle<'_, ArkaWm>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);

        let delta = (event.location - self.start_data.location).to_i32_round::<i32>();
        let mut rect = self.initial;
        if self.edges.contains(Edges::LEFT) {
            let w = (self.initial.size.w - delta.x).max(MIN_SIZE.0);
            rect.loc.x = self.initial.loc.x + self.initial.size.w - w;
            rect.size.w = w;
        } else if self.edges.contains(Edges::RIGHT) {
            rect.size.w = (self.initial.size.w + delta.x).max(MIN_SIZE.0);
        }
        if self.edges.contains(Edges::TOP) {
            let h = (self.initial.size.h - delta.y).max(MIN_SIZE.1);
            rect.loc.y = self.initial.loc.y + self.initial.size.h - h;
            rect.size.h = h;
        } else if self.edges.contains(Edges::BOTTOM) {
            rect.size.h = (self.initial.size.h + delta.y).max(MIN_SIZE.1);
        }
        if let Some(index) = data.index_of_window(&self.window) {
            data.windows[index].rect = Rectangle::new(rect.loc, Size::from((rect.size.w, rect.size.h)));
            data.arrange();
        }
    }

    fn relative_motion(
        &mut self,
        data: &mut ArkaWm,
        handle: &mut PointerInnerHandle<'_, ArkaWm>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &ButtonEvent) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, details: AxisFrame) {
        handle.axis(data, details)
    }

    fn frame(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureSwipeBeginEvent) {
        handle.gesture_swipe_begin(data, event)
    }
    fn gesture_swipe_update(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureSwipeUpdateEvent) {
        handle.gesture_swipe_update(data, event)
    }
    fn gesture_swipe_end(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureSwipeEndEvent) {
        handle.gesture_swipe_end(data, event)
    }
    fn gesture_pinch_begin(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GesturePinchBeginEvent) {
        handle.gesture_pinch_begin(data, event)
    }
    fn gesture_pinch_update(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GesturePinchUpdateEvent) {
        handle.gesture_pinch_update(data, event)
    }
    fn gesture_pinch_end(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GesturePinchEndEvent) {
        handle.gesture_pinch_end(data, event)
    }
    fn gesture_hold_begin(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureHoldBeginEvent) {
        handle.gesture_hold_begin(data, event)
    }
    fn gesture_hold_end(&mut self, data: &mut ArkaWm, handle: &mut PointerInnerHandle<'_, ArkaWm>, event: &GestureHoldEndEvent) {
        handle.gesture_hold_end(data, event)
    }

    fn start_data(&self) -> &PointerGrabStartData<ArkaWm> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut ArkaWm) {
        self.window.toplevel().unwrap().with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Resizing);
        });
        data.arrange();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rectangle<i32, Logical> {
        Rectangle::new((0, 0).into(), (1440, 900).into())
    }

    #[test]
    fn edges_and_corners_snap() {
        let s = screen();
        assert_eq!(snap_at(s, (0.0, 450.0).into()), Some(WindowState::Snapped(Snap::Left)));
        assert_eq!(snap_at(s, (1439.0, 450.0).into()), Some(WindowState::Snapped(Snap::Right)));
        assert_eq!(snap_at(s, (2.0, 10.0).into()), Some(WindowState::Snapped(Snap::TopLeft)));
        assert_eq!(snap_at(s, (1439.0, 899.0).into()), Some(WindowState::Snapped(Snap::BottomRight)));
        assert_eq!(snap_at(s, (700.0, 0.0).into()), Some(WindowState::Maximized));
        assert_eq!(snap_at(s, (700.0, 450.0).into()), None);
    }

    #[test]
    fn halves_fill_the_area_with_gaps() {
        let area = Rectangle::new((0, 40).into(), (1440, 860).into());
        let left = ArkaWm::snap_rect(Snap::Left, area);
        let right = ArkaWm::snap_rect(Snap::Right, area);
        let gap = crate::layout::GAP;
        assert_eq!(left.loc, (gap, 40 + gap).into());
        assert_eq!(left.size.h, 860 - 2 * gap);
        assert_eq!(right.loc.x, left.loc.x + left.size.w + gap);
        assert!(right.loc.x + right.size.w <= 1440 - gap);
        let br = ArkaWm::snap_rect(Snap::BottomRight, area);
        assert!(br.loc.y + br.size.h <= 40 + 860 - gap);
    }
}
