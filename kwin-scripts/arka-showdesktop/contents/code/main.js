// Meta+D: put every ordinary window away, and bring exactly those back on the next press.
// KDE's own Show Desktop hides the dock together with the windows (the dock is a layer-shell surface and
// KDE hides it too, whichever layer it sits on). Minimizing the windows leaves the dock where it is.
var away = [];

function onThisDesktop(w) {
    return w.onAllDesktops || w.desktops.indexOf(workspace.currentDesktop) >= 0;
}

function toggle() {
    if (away.length > 0) {
        var back = away;
        away = [];
        back.forEach(function (w) {
            try { w.minimized = false; } catch (e) { /* the window was closed meanwhile */ }
        });
        return;
    }
    workspace.windowList().forEach(function (w) {
        if (w.normalWindow && !w.minimized && onThisDesktop(w)) {
            away.push(w);
            w.minimized = true;
        }
    });
}

registerShortcut("ArkaOS Show Desktop", "Show the desktop and keep the dock", "Meta+D", toggle);
