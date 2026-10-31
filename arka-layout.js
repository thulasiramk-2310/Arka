// ArkaOS desktop layout for KDE Plasma — evokes the ArkaOS design:
//   • a slim top bar (launcher + clock + spaces + system tray) on every screen
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

// --- Top bar ----------------------------------------------------------------
// launcher | wordmark | spacer | search | status pill | spaces | tray | clock
// Plasma will not make a panel thinner than a minimum it works out from the font size
// (about two grid units): asking for 28 gives the minimum, not 28, and a test session with
// large fallback fonts showed 48. So this is a request, and the bar's real height follows the
// font. Setting a height above the minimum (64) does work.
var BAR_HEIGHT = 36;   // room for the round search button and the status pill
var OLD_BAR_HEIGHT = 34;

// The spaces switcher: the virtual desktops as a hairline ridge, one peak per desktop with the
// current one marked, a click switches. It hides itself while there is only one desktop.
function addSpaces(bar) {
    return bar.addWidget('org.arkaos.spaces');
}

// The clock sits at the right end: date and time on one line, kept apart by two spaces.
function addClock(bar) {
    var clock = bar.addWidget('org.kde.plasma.digitalclock');
    // Day/month names follow the locale; 12/24-hour is left at the locale default.
    clock.currentConfigGroup = ['Appearance'];
    clock.writeConfig('showDate', true);
    clock.writeConfig('dateDisplayFormat', 1);  // beside the time
    clock.writeConfig('dateFormat', 'custom');
    // The trailing space keeps the date and time apart: on a bar attached to the
    // screen edge Plasma's clock otherwise draws them touching ('Oct05:49').
    clock.writeConfig('customDateFormat', 'ddd d MMM  ');
    // The bar's own size, not the big stock clock.
    clock.writeConfig('autoFontAndSize', false);
    clock.writeConfig('fontSize', 12);
    return clock;
}

function addBar(screen, launcher) {
    var bar = new Panel;
    bar.screen = screen;
    bar.location = 'top';
    bar.height = BAR_HEIGHT;
    try { bar.floating = false; } catch (e) {}  // flat bar on the screen edge; the dock floats
    bar.addWidget(launcher);
    bar.addWidget('org.arkaos.brand');
    bar.addWidget('org.kde.plasma.panelspacer');
    bar.addWidget('org.arkaos.search');
    bar.addWidget('org.arkaos.statuspill');
    addSpaces(bar);
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
    addClock(bar);
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

// --- Bars made before the 28px bar with spaces ----------------------------------
// Once per screen: a top bar still at the old 34px becomes 28px (a height the
// user chose themselves is left alone), and a bar without a spaces switcher gets
// one just before the tray. The screen's desktop remembers it was done, so a
// switcher the user removes afterwards stays removed.
for (var m = 0; keepPanels && m < all.length; m++) {
    var old = all[m];
    var oldDesk = desktopForScreen(old.screen);
    if (old.location != 'top' || !oldDesk) {
        continue;
    }
    oldDesk.currentConfigGroup = ['General'];
    if (oldDesk.readConfig('arkaBarV2', '') == 'done') {
        continue;
    }
    if (old.height == OLD_BAR_HEIGHT) {
        old.height = BAR_HEIGHT;
    }
    var items = old.widgets();
    var hasSpaces = items.some(function (w) { return w.type == 'org.kde.plasma.pager' || w.type == 'org.arkaos.spaces'; });
    if (!hasSpaces) {
        var added = addSpaces(old);
        var trayItem = items.filter(function (w) { return w.type == 'org.kde.plasma.systemtray'; })[0];
        try { if (trayItem) { added.index = trayItem.index; } } catch (e) {}
    }
    oldDesk.currentConfigGroup = ['General'];
    oldDesk.writeConfig('arkaBarV2', 'done');
}
// A bar made just now already has it.
for (var n = 0; keepPanels && n < screenCount; n++) {
    var madeDesk = desktopForScreen(n);
    if (madeDesk) {
        madeDesk.currentConfigGroup = ['General'];
        madeDesk.writeConfig('arkaBarV2', 'done');
    }
}

// --- Bars made before the wordmark, search and status pill ------------------------
// Once per screen: an existing bar gets the wordmark after the launcher, the search button and the status
// pill before the spaces switcher, and its clock moves to the right end. Anything the user already
// removed, moved or added is kept, and a widget they remove later stays removed (the screen's desktop
// remembers this was done).
function indexOfType(bar, type) {
    var w = bar.widgets();
    for (var k = 0; k < w.length; k++) { if (w[k].type == type) { return k; } }
    return -1;
}
for (var q = 0; keepPanels && q < all.length; q++) {
    var bar3 = all[q];
    var deskQ = desktopForScreen(bar3.screen);
    if (bar3.location != 'top' || !deskQ) {
        continue;
    }
    deskQ.currentConfigGroup = ['General'];
    if (deskQ.readConfig('arkaBarV3', '') == 'done') {
        continue;
    }
    var types3 = bar3.widgets().map(function (w) { return w.type; });
    if (types3.indexOf('org.kde.plasma.systemtray') < 0) {
        continue;   // not one of ours
    }
    if (bar3.height < BAR_HEIGHT) { bar3.height = BAR_HEIGHT; }
    var li = -1;
    for (var m3 = 0; m3 < launchers.length && li < 0; m3++) { li = indexOfType(bar3, launchers[m3]); }
    var brand = bar3.addWidget('org.arkaos.brand');
    try { brand.index = li + 1; } catch (e) {}
    var anchorAt = indexOfType(bar3, 'org.arkaos.spaces');
    if (anchorAt < 0) { anchorAt = indexOfType(bar3, 'org.kde.plasma.pager'); }
    if (anchorAt < 0) { anchorAt = indexOfType(bar3, 'org.kde.plasma.systemtray'); }
    var sb = bar3.addWidget('org.arkaos.search');
    try { sb.index = anchorAt; } catch (e) {}
    var pill = bar3.addWidget('org.arkaos.statuspill');
    try { pill.index = anchorAt + 1; } catch (e) {}
    // One flexible spacer is enough now that the clock leaves the middle.
    var sp = bar3.widgets().filter(function (w) { return w.type == 'org.kde.plasma.panelspacer'; });
    for (var z = 1; z < sp.length; z++) { sp[z].remove(); }
    var clockAt = indexOfType(bar3, 'org.kde.plasma.digitalclock');
    if (clockAt >= 0) { try { bar3.widgets()[clockAt].index = bar3.widgets().length - 1; } catch (e) {} }
    deskQ.currentConfigGroup = ['General'];
    deskQ.writeConfig('arkaBarV3', 'done');
}
// A bar made just now already has them.
for (var n3 = 0; keepPanels && n3 < screenCount; n3++) {
    var madeDesk3 = desktopForScreen(n3);
    if (madeDesk3) {
        madeDesk3.currentConfigGroup = ['General'];
        madeDesk3.writeConfig('arkaBarV3', 'done');
    }
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
