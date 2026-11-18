use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::{FilterResult, Keysym, ModifiersState},
        pointer::{Focus, GrabStartData},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    utils::{Serial, SERIAL_COUNTER},
};

use crate::{
    grabs::{Edges, MoveGrab, ResizeGrab},
    state::{ArkaWm, Snap, WindowState},
};

// Button codes from linux/input-event-codes.h.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

impl ArkaWm {
    /// One key press or release, from the real keyboard or from the developer test hook: ArkaWM's
    /// shortcuts first, then whoever holds the keyboard (an input method, a menu, the focused app).
    pub fn key(&mut self, code: smithay::input::keyboard::Keycode, key_state: KeyState, time: u32) {
        let serial = SERIAL_COUNTER.next_serial();
        let pressed = key_state == KeyState::Pressed;
        let action = self.seat.get_keyboard().unwrap().input::<Action, _>(
            self,
            code,
            key_state,
            serial,
            time,
            |state, modifiers, handle| {
                // The user's own shortcuts first, then ArkaWM's.
                if pressed {
                    state.user_keys.refresh();
                }
                let found = state.user_keys.find(modifiers, handle.modified_sym()).map(Action::Custom);
                match found.or_else(|| shortcut(modifiers, handle.modified_sym())) {
                // Locked: every key goes to the lock screen, no shortcut works.
                // Ctrl+Alt+F1..F12 switches console, locked or not: it is the way back out.
                _ if (Keysym::XF86_Switch_VT_1.raw()..=Keysym::XF86_Switch_VT_12.raw()).contains(&handle.modified_sym().raw()) => {
                    if pressed {
                        let number = (handle.modified_sym().raw() - Keysym::XF86_Switch_VT_1.raw() + 1) as i32;
                        FilterResult::Intercept(Action::Console(number))
                    } else {
                        FilterResult::Intercept(Action::None)
                    }
                }
                // sound and brightness keep working on the lock screen, as on Plasma
                Some(Action::Media(key)) => FilterResult::Intercept(if pressed { Action::Media(key) } else { Action::None }),
                _ if state.locked => FilterResult::Forward,
                // Esc backs out of the window switcher and of picking a part of the screen.
                _ if (state.switcher.is_some() || state.region.is_some()) && handle.modified_sym() == Keysym::Escape => {
                    FilterResult::Intercept(if pressed { Action::Cancel } else { Action::None })
                }
                Some(action) if pressed => FilterResult::Intercept(action),
                // Swallow the release of a shortcut key too.
                Some(_) => FilterResult::Intercept(Action::None),
                None => FilterResult::Forward,
            }},
        );
        // a held volume or brightness key repeats until any key is let go or another is pressed
        if self.media_repeat.is_some() && (!pressed || !matches!(action, Some(Action::Media(_)))) {
            self.stop_media_repeat();
        }
        if let Some(action) = action {
            self.run(action);
        }
        // Letting go of Alt picks the window the switcher is on.
        if self.switcher.is_some() && !self.seat.get_keyboard().unwrap().modifier_state().alt {
            self.switcher_close(true);
        }
    }

    /// A media key: run its command here (so the keys work without the shell), then let the
    /// shell show what changed.
    fn media(&mut self, key: &'static str) {
        // run here, so the keys work even without the shell; the shell then shows the result
        let (command, shown) = match key {
            "volume-up" => ("wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+", Some("volume")),
            "volume-down" => ("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-", Some("volume")),
            "mute" => ("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle", Some("volume")),
            "mic-mute" => ("wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle", Some("mic")),
            // never all the way to black
            "brightness-up" => ("brightnessctl -q --class=backlight set 5%+", Some("brightness")),
            "brightness-down" => ("brightnessctl -q --class=backlight --min-value=2% set 5%-", Some("brightness")),
            "wifi" => ("if [ \"$(nmcli radio wifi)\" = enabled ]; then nmcli radio wifi off; else nmcli radio wifi on; fi", Some("wifi")),
            "play" => ("playerctl play-pause", None),
            "next" => ("playerctl next", None),
            _ => ("playerctl previous", None),
        };
        crate::state::spawn(&format!("{command} >/dev/null 2>&1"));
        if let Some(what) = shown {
            self.tell_shell(&format!("media {what}\n"));
        }
    }

    fn stop_media_repeat(&mut self) {
        if let Some(token) = self.media_repeat.take() {
            self.loop_handle.remove(token);
        }
    }

    /// The pointer moved to `pos` (logical pixels), from the real mouse or the developer test hook.
    pub fn pointer_move(&mut self, pos: smithay::utils::Point<f64, smithay::utils::Logical>, time: u32) {
        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        // While a part of the screen is being picked, apps do not see the pointer.
        let under = if let Some(region) = self.region.as_mut() {
            region.now = pos;
            self.redraw_wanted = true;
            None
        } else {
            self.surface_under(pos)
        };
        // light the title-bar button under the pointer
        let hover = if self.locked { None } else { self.bar_at(pos).and_then(|(index, button)| Some((index, button?))) };
        if hover != self.bar_hover {
            self.bar_hover = hover;
            self.redraw_wanted = true;
        }
        pointer.motion(self, under, &MotionEvent { location: pos, serial, time });
        pointer.frame(self);
    }

    /// A mouse button went down or up.
    pub fn pointer_button(&mut self, button: u32, button_state: ButtonState, time: u32) {
        let pointer = self.seat.get_pointer().unwrap();
        let serial = SERIAL_COUNTER.next_serial();

        // Picking a part of the screen for a screenshot: press, drag, let go.
        if self.region.is_some() && !self.locked {
            if button == BTN_LEFT {
                self.region_button(button_state == ButtonState::Pressed, pointer.current_location());
            }
            return;
        }

        // A click on a title bar ArkaWM drew: a button, maximise on a double click, or start moving.
        if ButtonState::Pressed == button_state && !pointer.is_grabbed() && !self.locked {
            if let Some((index, on_button)) = self.bar_at(pointer.current_location()) {
                if button != BTN_LEFT {
                    return;
                }
                self.redraw_wanted = true;
                match on_button {
                    Some(crate::titlebar::Button::Close) => {
                        self.windows[index].window.toplevel().unwrap().send_close();
                        return;
                    }
                    Some(crate::titlebar::Button::Minimize) => {
                        self.set_minimized(index, true);
                        return;
                    }
                    Some(crate::titlebar::Button::Maximize) => {
                        self.focus_index(index);
                        self.toggle_maximize_focused();
                        return;
                    }
                    None => {}
                }
                if index != self.focused {
                    self.focus_index(index);
                }
                let now = std::time::Instant::now();
                let double = matches!(self.last_bar_click, Some((at, i)) if i == index && now.duration_since(at).as_millis() < 400);
                self.last_bar_click = if double { None } else { Some((now, index)) };
                if double {
                    self.toggle_maximize_focused();
                    return;
                }
                let start_data = GrabStartData { focus: None, button, location: pointer.current_location() };
                let window = self.windows[index].window.clone();
                if let Some(grab) = MoveGrab::start(self, start_data, window) {
                    pointer.set_grab(self, grab, serial, Focus::Clear);
                }
                pointer.button(self, &ButtonEvent { button, state: button_state, serial, time });
                pointer.frame(self);
                return;
            }
        }

        // Clicking a window focuses it (and scrolls it fully into view).
        if ButtonState::Pressed == button_state && !pointer.is_grabbed() && !self.locked {
            let clicked = self
                .space
                .element_under(pointer.current_location())
                .and_then(|(w, _)| w.toplevel().map(|t| t.wl_surface().clone()))
                .and_then(|surface| self.index_of(&surface));
            // A click on a panel or the desktop that takes the keyboard (a menu, a sheet) gives
            // it the keyboard, so Tab, Enter and Escape work there.
            if clicked.is_none() {
                use smithay::wayland::shell::wlr_layer::{KeyboardInteractivity, LayerSurfaceCachedState};
                let wants_keys = self.surface_under(pointer.current_location()).map(|(surface, _)| {
                    let mut root = surface;
                    while let Some(parent) = smithay::wayland::compositor::get_parent(&root) {
                        root = parent;
                    }
                    root
                }).filter(|root| {
                    self.layer_shell_state.layer_surfaces().any(|layer| layer.wl_surface() == root)
                        && smithay::wayland::compositor::with_states(root, |states| {
                            states.cached_state.get::<LayerSurfaceCachedState>().current().keyboard_interactivity != KeyboardInteractivity::None
                        })
                });
                if let Some(root) = wants_keys {
                    self.focus_surface(Some(root), serial);
                }
            }
            if let Some(index) = clicked {
                if index != self.focused {
                    self.focus_index(index);
                }
                // Alt + left drag moves a window from anywhere in it,
                // Alt + right drag resizes it from the nearest corner.
                let alt = self.seat.get_keyboard().unwrap().modifier_state().alt;
                if alt && (button == BTN_LEFT || button == BTN_RIGHT) {
                    self.start_alt_grab(index, button, serial);
                }
            }
        };

        pointer.button(self, &ButtonEvent { button, state: button_state, serial, time });
        pointer.frame(self);
    }

    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        // Any key, click or pointer movement counts as the user being there.
        let seat = self.seat.clone();
        self.idle_notifier_state.notify_activity(&seat);

        match event {
            InputEvent::Keyboard { event, .. } => {
                self.key(event.key_code(), event.state(), Event::time_msec(&event));
            }
            // A mouse or touchpad: moves the pointer from where it is, kept on the screen.
            InputEvent::PointerMotion { event, .. } => {
                use smithay::backend::input::PointerMotionEvent;
                let area = self.output_area();
                let mut pos = self.seat.get_pointer().unwrap().current_location();
                pos.x = (pos.x + event.delta_x()).clamp(area.loc.x as f64, (area.loc.x + area.size.w - 1) as f64);
                pos.y = (pos.y + event.delta_y()).clamp(area.loc.y as f64, (area.loc.y + area.size.h - 1) as f64);
                self.pointer_move(pos, event.time_msec());
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();
                let output_geo = self.space.output_geometry(output).unwrap();
                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
                self.pointer_move(pos, event.time_msec());
            }
            InputEvent::PointerButton { event, .. } => {
                self.pointer_button(event.button_code(), event.state(), event.time_msec());
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event
                    .amount(Axis::Horizontal)
                    .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}

/// What a keyboard shortcut does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    None,
    Fullscreen,
    Terminal,
    Close,
    SnapLeft,
    SnapRight,
    Maximize,
    Restore,
    NextWindow,
    PreviousWindow,
    Cancel,
    /// The Windows key with Down: restore a maximised or snapped window, minimise a normal one.
    Down,
    ShowDesktop,
    /// The ArkaOS wheel: formats (false) or edits (true).
    Wheel(bool),
    Files,
    Settings,
    Screenshot,
    Region,
    /// Ask the shell to open one of its panels.
    Panel(&'static str),
    /// A media key on the keyboard: sound, microphone, brightness, Wi-Fi, playback.
    Media(&'static str),
    MoveLeft,
    MoveRight,
    CycleWidth,
    ToggleLayout,
    Lock,
    /// The system monitor (Task Manager on Windows).
    Monitor,
    /// A shortcut from the user's own file: its place in that list.
    Custom(usize),
    Console(i32),
    Quit,
}

/// ArkaWM's shortcuts. Nested in another desktop, Super belongs to that
/// desktop, so the modifier is Alt for now.
fn shortcut(modifiers: &ModifiersState, sym: Keysym) -> Option<Action> {
    // The keyboard's own keys for sound, brightness, Wi-Fi and playback.
    if !modifiers.ctrl && !modifiers.alt && !modifiers.logo {
        let media = match sym {
            Keysym::XF86_AudioRaiseVolume => Some("volume-up"),
            Keysym::XF86_AudioLowerVolume => Some("volume-down"),
            Keysym::XF86_AudioMute => Some("mute"),
            Keysym::XF86_AudioMicMute => Some("mic-mute"),
            Keysym::XF86_MonBrightnessUp => Some("brightness-up"),
            Keysym::XF86_MonBrightnessDown => Some("brightness-down"),
            Keysym::XF86_WLAN | Keysym::XF86_RFKill => Some("wifi"),
            Keysym::XF86_AudioPlay | Keysym::XF86_AudioPause => Some("play"),
            Keysym::XF86_AudioNext => Some("next"),
            Keysym::XF86_AudioPrev => Some("previous"),
            _ => None,
        };
        if let Some(key) = media {
            return Some(Action::Media(key));
        }
    }
    // Print Screen: the whole screen; with Shift, a part of it.
    if sym == Keysym::Print && !modifiers.ctrl && !modifiers.alt && !modifiers.logo {
        return Some(if modifiers.shift { Action::Region } else { Action::Screenshot });
    }
    // Windows key + Alt: the ArkaOS format and edit wheels (the same keys as on Plasma).
    if modifiers.logo && modifiers.alt && !modifiers.ctrl && !modifiers.shift {
        match sym {
            Keysym::c | Keysym::C => return Some(Action::Wheel(false)),
            Keysym::e | Keysym::E => return Some(Action::Wheel(true)),
            _ => {}
        }
    }
    // Ctrl+Alt+T opens a terminal (Ubuntu, KDE); Ctrl+Shift+Esc the system monitor (Windows).
    if modifiers.ctrl && !modifiers.logo {
        return match (modifiers.alt, modifiers.shift, sym) {
            (true, false, Keysym::t | Keysym::T) => Some(Action::Terminal),
            (false, true, Keysym::Escape) => Some(Action::Monitor),
            _ => None,
        };
    }
    // The Windows key, as on Windows.
    if modifiers.logo && !modifiers.ctrl && !modifiers.alt {
        return Some(match (modifiers.shift, sym) {
            (false, Keysym::d) => Action::ShowDesktop,
            (false, Keysym::e) => Action::Files,
            (false, Keysym::i) => Action::Settings,
            (false, Keysym::l) => Action::Lock,
            (false, Keysym::s) => Action::Panel("search"),
            (false, Keysym::Return) => Action::Terminal,
            (false, Keysym::Left) => Action::SnapLeft,
            (false, Keysym::Right) => Action::SnapRight,
            (false, Keysym::Up) => Action::Maximize,
            (false, Keysym::Down) => Action::Down,
            (true, Keysym::S | Keysym::s) => Action::Region,
            (false, Keysym::v) => Action::Panel("clipboard"),
            (false, Keysym::a) => Action::Panel("quick"),
            (false, Keysym::n) => Action::Panel("notifications"),
            _ => return None,
        });
    }
    if !modifiers.alt || modifiers.ctrl || modifiers.logo {
        return None;
    }
    let action = match (modifiers.shift, sym) {
        (false, Keysym::Return) => Action::Terminal,
        (false, Keysym::q | Keysym::F4) => Action::Close,
        (false, Keysym::Left) => Action::SnapLeft,
        (false, Keysym::Right) => Action::SnapRight,
        (false, Keysym::Up) => Action::Maximize,
        (false, Keysym::Down) => Action::Restore,
        (false, Keysym::f) => Action::Fullscreen,
        (false, Keysym::Tab) => Action::NextWindow,
        (true, Keysym::Tab | Keysym::ISO_Left_Tab) => Action::PreviousWindow,
        (true, Keysym::Left) => Action::MoveLeft,
        (true, Keysym::Right) => Action::MoveRight,
        (false, Keysym::r) => Action::CycleWidth,
        (false, Keysym::w) => Action::ToggleLayout,
        (false, Keysym::l) => Action::Lock,
        (true, Keysym::E | Keysym::e) => Action::Quit,
        _ => return None,
    };
    Some(action)
}

/// The locker: with the lock style the desktop shell writes from its theme, unless the user has a
/// swaylock config of their own or chose another locker with ARKAWM_LOCKER. arka-lock, the ArkaWM
/// lock screen, comes first when it is installed.
const LOCK: &str = r#"style="$XDG_RUNTIME_DIR/arka-desk-swaylock.conf"; if [ -n "$ARKAWM_LOCKER" ]; then exec $ARKAWM_LOCKER; elif command -v arka-lock >/dev/null; then exec arka-lock; elif [ -f "$style" ] && [ ! -e "${XDG_CONFIG_HOME:-$HOME/.config}/swaylock/config" ] && [ ! -e "$HOME/.swaylock/config" ]; then exec swaylock -C "$style"; else exec swaylock; fi"#;

impl ArkaWm {
    fn run(&mut self, action: Action) {
        self.redraw_wanted = true;
        tracing::debug!("shortcut {action:?}");
        match action {
            Action::None => {}
            Action::Terminal => crate::state::spawn("${ARKAWM_TERMINAL:-konsole}"),
            Action::Close => self.close_focused(),
            Action::Console(number) => self.switch_console(number),
            Action::Lock => crate::state::spawn(LOCK),
            Action::Monitor => crate::state::spawn(
                "${ARKAWM_MONITOR:-$(command -v plasma-systemmonitor || command -v gnome-system-monitor || echo 'konsole -e top')}",
            ),
            Action::Custom(index) => {
                if let Some(command) = self.user_keys.command(index) {
                    tracing::info!("your shortcut: {command}");
                    crate::state::spawn(command);
                }
            }
            Action::SnapLeft => self.snap_focused(Snap::Left),
            Action::SnapRight => self.snap_focused(Snap::Right),
            Action::Fullscreen => self.toggle_fullscreen_focused(),
            Action::Maximize => self.toggle_maximize_focused(),
            Action::Restore => self.set_state(self.focused, WindowState::Normal),
            Action::NextWindow => self.switcher_step(false),
            Action::PreviousWindow => self.switcher_step(true),
            Action::Cancel => {
                self.switcher_close(false);
                self.region = None;
            }
            Action::Down => match self.windows.get(self.focused).map(|m| (m.state, m.minimized)) {
                Some((WindowState::Normal, false)) => self.set_minimized(self.focused, true),
                Some((_, false)) => self.set_state(self.focused, WindowState::Normal),
                _ => {}
            },
            Action::ShowDesktop => self.toggle_show_desktop(),
            Action::Files => crate::state::spawn("${ARKAWM_FILES:-dolphin}"),
            // Plasma reaches the wheel through its shortcut service; here the wheel is asked over
            // the session bus. Nothing happens when the wheel is not running.
            Action::Wheel(edit) => crate::state::spawn(&format!(
                "gdbus call --session --dest org.arkaos.Wheel --object-path /Wheel --method org.arkaos.Wheel.{} >/dev/null 2>&1",
                if edit { "ShowEditForDrag" } else { "ShowForDrag" }
            )),
            Action::Settings => crate::state::spawn("${ARKAWM_SETTINGS:-$(command -v arka-settings || echo systemsettings)}"),
            Action::Screenshot => self.capture = Some(crate::capture::Capture { crop: None }),
            Action::Region => {
                let now = self.seat.get_pointer().unwrap().current_location();
                self.region = Some(crate::capture::Region { start: None, now });
            }
            Action::Panel(name) => self.tell_shell(&format!("show {name}\n")),
            Action::Media(key) => {
                self.media(key);
                if matches!(key, "volume-up" | "volume-down" | "brightness-up" | "brightness-down") {
                    self.stop_media_repeat();
                    use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
                    let timer = Timer::from_duration(std::time::Duration::from_millis(400));
                    self.media_repeat = self
                        .loop_handle
                        .insert_source(timer, move |_, _, state| {
                            state.media(key);
                            TimeoutAction::ToDuration(std::time::Duration::from_millis(90))
                        })
                        .ok();
                }
            }
            Action::MoveLeft => self.move_focused(-1),
            Action::MoveRight => self.move_focused(1),
            Action::CycleWidth => self.cycle_width(),
            Action::ToggleLayout => self.toggle_mode(),
            Action::Quit => self.loop_signal.stop(),
        }
    }

    fn start_alt_grab(&mut self, index: usize, button: u32, serial: Serial) {
        let pointer = self.seat.get_pointer().unwrap();
        let location = pointer.current_location();
        let start_data = GrabStartData { focus: None, button, location };
        let window = self.windows[index].window.clone();
        if button == BTN_LEFT {
            if let Some(grab) = MoveGrab::start(self, start_data, window) {
                pointer.set_grab(self, grab, serial, Focus::Clear);
            }
        } else {
            // Resize from the corner nearest the pointer.
            let Some(loc) = self.space.element_location(&window) else { return };
            let size = window.geometry().size;
            let mut edges = Edges::empty();
            edges |= if location.x < (loc.x + size.w / 2) as f64 { Edges::LEFT } else { Edges::RIGHT };
            edges |= if location.y < (loc.y + size.h / 2) as f64 { Edges::TOP } else { Edges::BOTTOM };
            if let Some(grab) = ResizeGrab::start(self, start_data, window, edges) {
                pointer.set_grab(self, grab, serial, Focus::Clear);
            }
        }
    }
}
