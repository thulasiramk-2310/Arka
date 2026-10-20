// When screens are added or removed, start arka-panels, which gives a new
// screen the ArkaOS top bar (see /usr/libexec/arka-panels).
workspace.screensChanged.connect(function () {
    callDBus("org.freedesktop.systemd1", "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager", "StartUnit", "arka-panels.service", "replace");
});
