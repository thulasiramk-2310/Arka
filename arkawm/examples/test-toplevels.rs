//! Test client for ArkaWM's window list (wlr-foreign-toplevel-management), the protocol Crystal
//! Dock uses. Prints what it is told about each window, then brings the first window that is not
//! in front forward, and prints again.
//!
//!     ARKAWM_TRUST_WINDOWS=$PWD/target/debug/examples/test-toplevels arkawm -c "..."

use std::collections::HashMap;
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_output, wl_registry, wl_seat},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

#[derive(Default, Debug)]
struct Win {
    title: String,
    app_id: String,
    states: Vec<u32>,
    outputs: usize,
    closed: bool,
}

#[derive(Default)]
struct App {
    windows: HashMap<u32, (ZwlrForeignToplevelHandleV1, Win)>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_output::WlOutput, ()> for App {
    fn event(_: &mut Self, _: &wl_output::WlOutput, _: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for App {
    fn event(app: &mut Self, _: &ZwlrForeignToplevelManagerV1, event: zwlr_foreign_toplevel_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            app.windows.insert(toplevel.id().protocol_id(), (toplevel, Win::default()));
        }
    }
    wayland_client::event_created_child!(App, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for App {
    fn event(app: &mut Self, handle: &ZwlrForeignToplevelHandleV1, event: zwlr_foreign_toplevel_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwlr_foreign_toplevel_handle_v1::Event;
        let Some((_, win)) = app.windows.get_mut(&handle.id().protocol_id()) else { return };
        match event {
            Event::Title { title } => win.title = title,
            Event::AppId { app_id } => win.app_id = app_id,
            Event::State { state } => win.states = state.chunks(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect(),
            Event::OutputEnter { .. } => win.outputs += 1,
            Event::OutputLeave { .. } => win.outputs -= 1,
            Event::Closed => win.closed = true,
            _ => {}
        }
    }
}

fn print(app: &App, when: &str) {
    println!("-- {when}");
    let mut list: Vec<_> = app.windows.values().map(|(_, w)| w).collect();
    list.sort_by(|a, b| a.title.cmp(&b.title));
    for w in list {
        println!("window app_id={:?} title={:?} states={:?} outputs={} closed={}", w.app_id, w.title, w.states, w.outputs, w.closed);
    }
}

fn main() {
    let conn = Connection::connect_to_env().expect("no Wayland display");
    let (globals, mut queue) = registry_queue_init::<App>(&conn).unwrap();
    let qh = queue.handle();
    let Ok(_manager) = globals.bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ()) else {
        println!("no window list offered to this program");
        return;
    };
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=1, ()).unwrap();
    let _output = globals.bind::<wl_output::WlOutput, _, _>(&qh, 1..=4, ());
    let mut app = App::default();
    for _ in 0..3 {
        queue.roundtrip(&mut app).unwrap();
    }
    print(&app, "listed");
    let back = app.windows.values().find(|(_, w)| !w.states.contains(&2)).map(|(h, _)| h.clone());
    if let Some(handle) = back {
        handle.activate(&seat);
        std::thread::sleep(std::time::Duration::from_millis(300));
        for _ in 0..3 {
            queue.roundtrip(&mut app).unwrap();
        }
        print(&app, "after activating the window behind");
    }
}
