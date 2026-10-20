// ArkaOS desktop layout for KDE Plasma — evokes the ArkaOS design:
//   • a slim top bar (launcher + clock + system tray) on every screen
//   • the dock at the bottom is Crystal Dock, started separately
//   • the ArkaOS launcher (org.arkaos.launcher, plasma-launcher/), a popup
//     from the top-left button
// Applied once on first login: new desktops get it from the ArkaOS global
// theme, older ones from arka-plasma-firstrun via evaluateScript. arka-panels
// runs it again at every login and when a screen is added, with
// arkaKeepPanels set, to add a bar to a screen that doesn't have one yet.
// JS strings use single quotes so the whole script can be passed inside a
// double-quoted shell argument.

var keepPanels = typeof arkaKeepPanels !== 'undefined';

// The object that holds a tray's entries: the tray itself on Plasma 6.7,
// a separate containment named by SystrayContainmentId on older versions.
function trayItems(tray) {
    tray.currentConfigGroup = [];
    var id = tray.readConfig('SystrayContainmentId', '');
    return id !== '' ? desktopById(id) : tray;
}

// --- Top bar (34px) ---------------------------------------------------------
// launcher | spacer | clock (centred) | spacer | tray
function addBar(screen, launcher) {
    var bar = new Panel;
    bar.screen = screen;
    bar.location = 'top';
    bar.height = 34;
    try { bar.floating = false; } catch (e) {}  // flat bar on the screen edge; the dock floats
    bar.addWidget(launcher);
    bar.addWidget('org.kde.plasma.panelspacer');
    var clock = bar.addWidget('org.kde.plasma.digitalclock');
    // Readable date beside the time ('Sat 4 Oct 19:33'). Day/month names follow
    // the locale; 12/24-hour is left at the locale default (use24hFormat unset).
    clock.currentConfigGroup = ['Appearance'];
    clock.writeConfig('showDate', true);
    clock.writeConfig('dateDisplayFormat', 1);  // beside the time
    clock.writeConfig('dateFormat', 'custom');
    // The trailing space keeps the date and time apart: on a bar attached to the
    // screen edge Plasma's clock otherwise draws them touching ('Oct05:49').
    clock.writeConfig('customDateFormat', 'ddd d MMM ');
    bar.addWidget('org.kde.plasma.panelspacer');
    var tray = bar.addWidget('org.kde.plasma.systemtray');
    // Leave out tray entries a new account doesn't need: KDE Connect (off by
    // default, see Containerfile), Weather (offers to set up a weather service)
    // and Vaults. All can be switched back on in the tray's settings.
    // knownItems lists them as already seen, so Plasma doesn't add them back.
    var trayBox = trayItems(tray);
    if (trayBox) {
        var shown = ['org.kde.kscreen', 'org.kde.plasma.battery',
            'org.kde.plasma.bluetooth', 'org.kde.plasma.brightness', 'org.kde.plasma.cameraindicator',
            'org.kde.plasma.clipboard', 'org.kde.plasma.devicenotifier', 'org.kde.plasma.keyboardindicator',
            'org.kde.plasma.keyboardlayout', 'org.kde.plasma.manage-inputmethod',
            'org.kde.plasma.mediacontroller', 'org.kde.plasma.networkmanagement',
            'org.kde.plasma.notifications', 'org.kde.plasma.printmanager', 'org.kde.plasma.volume'];
        trayBox.currentConfigGroup = ['General'];
        trayBox.writeConfig('extraItems', shown);
        trayBox.writeConfig('knownItems',
            shown.concat(['org.kde.kdeconnect', 'org.kde.plasma.vault', 'org.kde.plasma.weather']));
    }
    return bar;
}

// A bar on another screen uses the launcher the account already has on its
// bars, so an account that kept KDE's menu gets the same one everywhere.
var launchers = ['org.arkaos.launcher', 'org.kde.plasma.kickerdash',
    'org.kde.plasma.kickoff', 'org.kde.plasma.kicker'];
var launcher = 'org.arkaos.launcher';
var all = panels();
for (var i = 0; keepPanels && i < all.length; i++) {
    var found = all[i].widgets().filter(function (w) { return launchers.indexOf(w.type) >= 0; });
    if (all[i].location == 'top' && found.length > 0) {
        launcher = found[0].type;
        break;
    }
}

// Each screen gets a bar once. The screen's desktop remembers it, so a bar
// someone removes from a screen is not added back.
var made = [];
for (var s = 0; s < screenCount; s++) {
    var desk = desktopForScreen(s);
    if (!desk) {
        continue;
    }
    desk.currentConfigGroup = ['General'];
    if (keepPanels && desk.readConfig('arkaTopBar', '') == 'added') {
        continue;
    }
    var hasBar = keepPanels && all.some(function (p) { return p.screen == s && p.location == 'top'; });
    if (!hasBar) {
        made.push(addBar(s, launcher).id);
    }
    desk.currentConfigGroup = ['General'];
    desk.writeConfig('arkaTopBar', 'added');
}

// --- Dock ---------------------------------------------------------------------
// The dock is Crystal Dock, its own window started at login (see crystal-dock/),
// not a Plasma panel: a panel can't grow icons above itself the way the macOS
// Dock does.

// --- Remove the original default panel(s) -----------------------------------
// Only on first login; later runs never remove a panel.
for (var j = 0; !keepPanels && j < all.length; j++) {
    if (made.indexOf(all[j].id) < 0) {
        all[j].remove();
    }
}
