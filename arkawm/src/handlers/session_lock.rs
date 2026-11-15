//! Screen lock and idle.
//!
//! The lock screen is a separate program (the "locker", swaylock for example) that asks ArkaWM to
//! lock through ext-session-lock. From that request until the locker says unlock:
//!   - only the locker's surface is drawn, on a plain background (no window, panel or dock);
//!   - the keyboard and pointer go only to that surface; no shortcut works;
//!   - no window can take the focus or open a menu.
//! If the locker dies while locked, the screen stays locked and blank, as the protocol requires.
//!
//! Idle: ArkaWM reports "no input for N seconds" to programs that ask (ext-idle-notify), which is
//! how an idle daemon knows when to start the locker. A playing video can hold that off
//! (idle-inhibit).

use smithay::{
    input::pointer::MotionEvent,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    utils::SERIAL_COUNTER,
    wayland::{
        idle_inhibit::IdleInhibitHandler,
        idle_notify::{IdleNotifierHandler, IdleNotifierState},
        session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
    },
};

use crate::ArkaWm;

impl SessionLockHandler for ArkaWm {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        let serial = SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        let pointer = self.seat.get_pointer().unwrap();
        // Drop any drag or open menu, then take the keyboard and pointer away from every window.
        pointer.unset_grab(self, serial, 0);
        keyboard.unset_grab(self);
        keyboard.set_focus(self, None, serial);
        let location = pointer.current_location();
        pointer.motion(self, None, &MotionEvent { location, serial, time: 0 });
        pointer.frame(self);

        self.locked = true;
        self.lock_surface = None;
        tracing::info!("lock requested");
        // Confirmed from the draw loop, after the first frame without windows.
        self.lock_pending = Some(confirmation);
    }

    fn unlock(&mut self) {
        self.locked = false;
        self.lock_surface = None;
        self.lock_pending = None;
        tracing::info!("screen unlocked");
        if !self.windows.is_empty() {
            self.focus_index(self.focused.min(self.windows.len() - 1));
        }
    }

    fn new_surface(&mut self, surface: LockSurface, _output: WlOutput) {
        let size = self.output_area().size;
        surface.with_pending_state(|state| {
            state.size = Some((size.w.max(1) as u32, size.h.max(1) as u32).into());
        });
        surface.send_configure();
        let serial = SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, Some(surface.wl_surface().clone()), serial);
        tracing::info!("lock screen surface created");
        self.lock_surface = Some(surface);
    }
}
smithay::delegate_session_lock!(ArkaWm);

impl IdleNotifierHandler for ArkaWm {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Self> {
        &mut self.idle_notifier_state
    }
}
smithay::delegate_idle_notify!(ArkaWm);

impl IdleInhibitHandler for ArkaWm {
    fn inhibit(&mut self, surface: WlSurface) {
        if !self.idle_inhibitors.contains(&surface) {
            self.idle_inhibitors.push(surface);
        }
        self.idle_notifier_state.set_is_inhibited(true);
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.idle_inhibitors.retain(|s| s != &surface);
        let held = !self.idle_inhibitors.is_empty();
        self.idle_notifier_state.set_is_inhibited(held);
    }
}
smithay::delegate_idle_inhibit!(ArkaWm);
