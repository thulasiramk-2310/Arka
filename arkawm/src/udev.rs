//! Running directly on the hardware, with no other desktop underneath: ArkaWM takes the screen
//! from the kernel (DRM), the keyboard, mouse and touchpad from libinput, and permission for both
//! from the login session (libseat).
//!
//! First version, deliberately small: one GPU, the first connected screen at its preferred mode,
//! a plain arrow pointer, a new frame on every screen refresh. Not yet: several screens or GPUs,
//! plugging a screen in while running, apps' own cursor pictures, touch and gestures.

use std::{path::PathBuf, time::Duration};

use smithay::{
    backend::{
        allocator::{
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc,
        },
        drm::{DrmDevice, DrmDeviceFd, DrmEvent, GbmBufferedSurface},
        egl::{EGLContext, EGLDevice, EGLDisplay},
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{damage::OutputDamageTracker, gles::GlesRenderer, Bind, ImportDma},
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
        udev::{all_gpus, primary_gpu},
    },
    output::{Mode as OutputMode, Output, PhysicalProperties, Scale as OutputScale, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop,
        },
        drm::control::{connector, crtc, Device as ControlDevice, ModeTypeFlags},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::{DeviceFd, Transform},
    wayland::dmabuf::{DmabufFeedbackBuilder, DmabufState},
};

use crate::ArkaWm;

type Surface = GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>;

/// Everything ArkaWM holds while it owns the screen.
/// Night light: how much of each colour is kept, about a 4000 K white (Redshift's table).
const WARM: (f64, f64, f64) = (1.0, 0.8128, 0.6339);

impl Udev {
    /// Make the screen warmer, or put its colours back, through the display pipe's gamma table.
    pub fn set_warmth(&self, on: bool) {
        let fd = self.drm.device_fd();
        let size = match fd.get_crtc(self.crtc) {
            Ok(info) => info.gamma_length() as usize,
            Err(err) => return tracing::warn!("night light: screen not readable: {err}"),
        };
        if size < 2 {
            return tracing::warn!("night light: this screen has no gamma table");
        }
        let (r, g, b) = if on { WARM } else { (1.0, 1.0, 1.0) };
        let ramp = |gain: f64| -> Vec<u16> {
            (0..size).map(|i| (i as f64 / (size - 1) as f64 * gain * 65535.0).round() as u16).collect()
        };
        match fd.set_gamma(self.crtc, &ramp(r), &ramp(g), &ramp(b)) {
            Ok(()) => tracing::info!("night light {}", if on { "on" } else { "off" }),
            Err(err) => tracing::warn!("night light could not be set: {err}"),
        }
    }
}

pub struct Udev {
    pub renderer: GlesRenderer,
    // the surface is dropped before the device it belongs to
    surface: Surface,
    drm: DrmDevice,
    /// The display pipe driving the screen, for its gamma table (night light).
    crtc: crtc::Handle,
    output: Output,
    tracker: OutputDamageTracker,
    /// A frame is on its way to the screen; the next one waits for the screen's refresh.
    waiting: bool,
    /// False while another console has the screen (after Ctrl+Alt+F-key).
    active: bool,
    /// Frames sent to the screen, and frames looked at but unchanged, since the last log line.
    shown: u32,
    unchanged: u32,
    /// How long one screen refresh takes.
    refresh: Duration,
    handle: smithay::reexports::calloop::LoopHandle<'static, ArkaWm>,
    /// Last, so it is dropped last: the screen is handed back while ArkaWM still has permission.
    pub session: LibSeatSession,
}

/// The first connected screen: its connector, a display controller that can drive it, and its preferred mode.
fn pick_screen(fd: &DrmDeviceFd) -> Option<(connector::Info, crtc::Handle, smithay::reexports::drm::control::Mode)> {
    let resources = fd.resource_handles().ok()?;
    resources.connectors().iter().find_map(|handle| {
        let info = fd.get_connector(*handle, true).ok()?;
        if info.state() != connector::State::Connected {
            return None;
        }
        let mode = *info
            .modes()
            .iter()
            .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .or_else(|| info.modes().first())?;
        let crtc = info.encoders().iter().find_map(|encoder| {
            let encoder = fd.get_encoder(*encoder).ok()?;
            resources.filter_crtcs(encoder.possible_crtcs()).first().copied()
        })?;
        Some((info, crtc, mode))
    })
}

pub fn init_udev(event_loop: &mut EventLoop<'static, ArkaWm>, state: &mut ArkaWm) -> Result<(), Box<dyn std::error::Error>> {
    let (mut session, notifier) = LibSeatSession::new()?;
    let seat = session.seat();

    // `ARKAWM_GPU=/dev/dri/card1` picks a GPU by hand on machines with two.
    let gpu = match std::env::var_os("ARKAWM_GPU") {
        Some(path) => PathBuf::from(path),
        None => primary_gpu(&seat)?
            .or_else(|| all_gpus(&seat).ok().and_then(|gpus| gpus.into_iter().next()))
            .ok_or("no GPU found")?,
    };
    tracing::info!("using GPU {}", gpu.display());

    let fd = session.open(&gpu, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (mut drm, drm_events) = DrmDevice::new(fd.clone(), true)?;
    let gbm = GbmDevice::new(fd.clone())?;
    let egl = unsafe { EGLDisplay::new(gbm.clone())? };
    let context = EGLContext::new(&egl)?;
    let renderer = unsafe { GlesRenderer::new(context)? };

    let (connector, crtc, mode) = pick_screen(&fd).ok_or("no connected screen found")?;
    let name = format!("{}-{}", connector.interface().as_str(), connector.interface_id());
    tracing::info!("screen {name}: {}x{} @ {} Hz", mode.size().0, mode.size().1, mode.vrefresh());

    let drm_surface = drm.create_surface(crtc, mode, &[connector.handle()])?;
    let allocator = GbmAllocator::new(gbm, GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let formats = renderer.egl_context().dmabuf_render_formats().clone();
    let surface: Surface = GbmBufferedSurface::new(drm_surface, allocator, &[Fourcc::Argb8888, Fourcc::Xrgb8888], formats)?;

    let (width_mm, height_mm) = connector.size().unwrap_or((0, 0));
    // No scale asked for: choose one from how dense the screen is.
    if std::env::var_os("ARKAWM_SCALE").is_none() {
        state.scale = crate::state::scale_for_screen(mode.size().0 as i32, width_mm as i32);
    }
    tracing::info!("scale {}", state.scale);
    let output = Output::new(
        name,
        PhysicalProperties {
            size: (width_mm as i32, height_mm as i32).into(),
            subpixel: Subpixel::Unknown,
            make: "ArkaOS".into(),
            model: "ArkaWM".into(),
        },
    );
    let display = state.display_handle.clone();
    let _global = output.create_global::<ArkaWm>(&display);
    let output_mode = OutputMode::from(mode);
    output.change_current_state(
        Some(output_mode),
        Some(Transform::Normal),
        Some(OutputScale::Fractional(state.scale)),
        Some((0, 0).into()),
    );
    output.set_preferred(output_mode);
    state.space.map_output(&output, (0, 0));

    // GPU buffers from apps (linux-dmabuf), as in the nested backend.
    let dmabuf_formats = renderer.dmabuf_formats();
    let render_node = EGLDevice::device_for_display(&egl).ok().and_then(|device| device.try_get_render_node().ok().flatten());
    let mut dmabuf_state = DmabufState::new();
    let dmabuf_global = match render_node
        .and_then(|node| DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats.clone()).build().ok())
    {
        Some(feedback) => dmabuf_state.create_global_with_default_feedback::<ArkaWm>(&display, &feedback),
        None => dmabuf_state.create_global::<ArkaWm>(&display, dmabuf_formats),
    };
    state.dmabuf = Some((dmabuf_state, dmabuf_global));

    // Keyboard, mouse and touchpad.
    let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput.udev_assign_seat(&seat).map_err(|_| "libinput could not take the seat")?;
    event_loop.handle().insert_source(LibinputInputBackend::new(libinput.clone()), |mut event, _, state| {
        if let InputEvent::DeviceAdded { device } = &mut event {
            // Tap to click, as people expect from a laptop touchpad.
            let _ = device.config_tap_set_enabled(true);
        }
        state.redraw_wanted = true;
        state.process_input_event(event);
    })?;

    // Switching away to another console and back.
    event_loop.handle().insert_source(notifier, move |event, _, state| {
        let Some(udev) = state.udev.as_mut() else { return };
        match event {
            SessionEvent::PauseSession => {
                libinput.suspend();
                udev.drm.pause();
                udev.active = false;
            }
            SessionEvent::ActivateSession => {
                if libinput.resume().is_err() {
                    tracing::warn!("input devices did not come back");
                }
                if let Err(err) = udev.drm.activate(false) {
                    tracing::warn!("screen did not come back: {err}");
                }
                udev.surface.reset_buffers();
                // the other console may have left its own colours on the screen
                udev.set_warmth(state.night);
                udev.active = true;
                udev.waiting = false;
                state.redraw_wanted = true;
            }
        }
    })?;

    // The screen has shown a frame: draw the next.
    event_loop.handle().insert_source(drm_events, |event, _, state| match event {
        DrmEvent::VBlank(_) => {
            if let Some(udev) = state.udev.as_mut() {
                if let Err(err) = udev.surface.frame_submitted() {
                    tracing::warn!("frame bookkeeping failed: {err}");
                }
                udev.waiting = false;
            }
        }
        DrmEvent::Error(err) => tracing::warn!("screen error: {err}"),
    })?;

    // Safety net: look again twice a second even if nothing asked, so a missed change cannot
    // leave a stale picture on screen for long.
    event_loop.handle().insert_source(Timer::from_duration(Duration::from_millis(500)), |_, _, state| {
        state.redraw_wanted = true;
        TimeoutAction::ToDuration(Duration::from_millis(500))
    })?;
    // A line in the log every ten seconds, to see how much drawing really happens.
    event_loop.handle().insert_source(Timer::from_duration(Duration::from_secs(10)), |_, _, state| {
        if let Some(udev) = state.udev.as_mut() {
            tracing::info!("last 10 s: {} frames shown, {} unchanged", udev.shown, udev.unchanged);
            (udev.shown, udev.unchanged) = (0, 0);
        }
        TimeoutAction::ToDuration(Duration::from_secs(10))
    })?;

    let tracker = OutputDamageTracker::from_output(&output);
    let refresh = Duration::from_secs_f64(1.0 / mode.vrefresh().max(30) as f64);
    let handle = event_loop.handle();
    state.udev = Some(Box::new(Udev { renderer, drm, crtc, surface, output, tracker, waiting: false, active: true, shown: 0, unchanged: 0, refresh, handle, session }));
    std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
    crate::state::center_pointer(state);
    state.redraw_wanted = true;
    Ok(())
}

/// Called after every round of events: draw a frame if one was asked for and the screen is free.
pub fn draw_if_wanted(state: &mut ArkaWm) {
    if state.redraw_wanted && state.udev.as_ref().is_some_and(|udev| udev.active && !udev.waiting) {
        draw(state);
    }
}

/// Draw one frame and, if anything in it changed, send it to the screen.
/// A frame that fails is skipped, never fatal.
fn draw(state: &mut ArkaWm) {
    // Taken out of the state while drawing, so both can be used at once.
    let Some(mut udev) = state.udev.take() else { return };
    state.redraw_wanted = false;
    let shot = state.shot_request.take();
    let capture = state.capture.take();
    // Ok(Some(true)): a new picture is on its way to the screen. Ok(Some(false)): nothing changed.
    let result = (|| -> Result<(bool, bool), Box<dyn std::error::Error>> {
        let (mut dmabuf, age) = udev.surface.next_buffer()?;
        let frame = crate::render::frame(state, &mut udev.renderer, &udev.output, true);
        if let Some(path) = shot {
            let size = udev.output.current_mode().map(|mode| mode.size).unwrap_or_default();
            crate::render::save_shot(&mut udev.renderer, &udev.output, size, &frame, &path, false);
        }
        // A screenshot for the user never has the pointer in it.
        if let Some(capture) = capture {
            let size = udev.output.current_mode().map(|mode| mode.size).unwrap_or_default();
            let clean = crate::render::frame(state, &mut udev.renderer, &udev.output, false);
            if let Some(rgba) = crate::render::grab(&mut udev.renderer, &udev.output, size, &clean, false) {
                state.finish_capture(capture, rgba, size);
            }
        }
        let changed = {
            let mut target = udev.renderer.bind(&mut dmabuf)?;
            udev.tracker
                .render_output(&mut udev.renderer, &mut target, age as usize, &frame.elements, frame.clear)
                .map_err(|err| format!("{err:?}"))?
                .damage
                .is_some()
        };
        if changed {
            udev.surface.queue_buffer(None, None, ())?;
        }
        Ok((changed, frame.locked))
    })();
    let output = udev.output.clone();
    let drawn = match result {
        Ok((changed, locked)) => {
            udev.waiting = true;
            if changed {
                udev.shown += 1;
            } else {
                udev.unchanged += 1;
                // Nothing went to the screen, so no refresh signal will come back. Wait one refresh
                // by the clock instead, so apps asking for frames are still paced.
                let timer = Timer::from_duration(udev.refresh);
                let armed = udev.handle.insert_source(timer, |_, _, state| {
                    if let Some(udev) = state.udev.as_mut() {
                        udev.waiting = false;
                    }
                    TimeoutAction::Drop
                });
                if armed.is_err() {
                    udev.waiting = false;
                }
            }
            Some(locked)
        }
        Err(err) => {
            tracing::warn!("frame not drawn: {err}");
            None
        }
    };
    state.udev = Some(udev);
    if let Some(locked) = drawn {
        crate::render::after_frame(state, &output, locked);
    }
}

impl ArkaWm {
    /// Ctrl+Alt+F-key: hand the screen to another console.
    pub fn switch_console(&mut self, number: i32) {
        if let Some(udev) = self.udev.as_mut() {
            if let Err(err) = udev.session.change_vt(number) {
                tracing::warn!("could not switch console: {err}");
            }
        }
    }
}
