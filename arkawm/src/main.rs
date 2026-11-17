#![allow(irrefutable_let_patterns)]

mod handlers;

mod cursor;
mod grabs;
mod input;
mod keys;
mod layout;
mod capture;
mod ipc;
mod render;
mod switcher;
mod state;
mod titlebar;
#[cfg(debug_assertions)]
mod testkeys;
mod trust;
mod udev;
mod winit;
mod xwayland;

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::Display,
};
pub use state::ArkaWm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let mut event_loop: EventLoop<'static, ArkaWm> = EventLoop::try_new()?;

    let display: Display<ArkaWm> = Display::new()?;
    let mut state = ArkaWm::new(&mut event_loop, display);

    // Inside another desktop ArkaWM runs as a window; from a text console it takes the screen itself.
    // `ARKAWM_BACKEND=winit` or `=udev` overrides the guess.
    let nested = match std::env::var("ARKAWM_BACKEND").as_deref() {
        Ok("udev") => false,
        Ok("winit") => true,
        _ => std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some(),
    };
    if nested {
        crate::winit::init_winit(&mut event_loop, &mut state)?;
    } else {
        crate::udev::init_udev(&mut event_loop, &mut state)?;
    }

    // The link to the desktop shell (window list, show desktop, screenshots).
    ipc::start(&event_loop.handle(), &state.socket_name);

    // X11 apps: started before any `-c` program so they inherit DISPLAY. Stopped when this is dropped.
    let _xwayland = xwayland::start(&state.socket_name);

    // Programs to start inside ArkaWM: each `-c <command>` (run through sh).
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "-c" || flag == "--command" {
            if let Some(command) = args.next() {
                state::spawn(&command);
            }
        }
    }

    #[cfg(debug_assertions)]
    testkeys::install(&event_loop.handle());

    // Safety net for first runs on hardware: `ARKAWM_EXIT_AFTER=20` quits after 20 seconds whatever
    // happens, so a black screen or dead keyboard cannot trap anyone.
    if let Some(seconds) = std::env::var("ARKAWM_EXIT_AFTER").ok().and_then(|v| v.parse::<u64>().ok()) {
        let timer = smithay::reexports::calloop::timer::Timer::from_duration(std::time::Duration::from_secs(seconds));
        let _ = event_loop.handle().insert_source(timer, |_, _, state| {
            tracing::info!("ARKAWM_EXIT_AFTER reached, quitting");
            state.loop_signal.stop();
            smithay::reexports::calloop::timer::TimeoutAction::Drop
        });
    }

    // After each round of events: on hardware, draw a frame if anything asked for one.
    event_loop.run(None, &mut state, move |state| {
        state.publish_windows();
        crate::udev::draw_if_wanted(state)
    })?;

    Ok(())
}
