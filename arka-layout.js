// ArkaOS desktop layout for KDE Plasma — evokes the ArkaOS design:
//   • a slim top bar (launcher + clock + system tray)
//   • the dock at the bottom is Crystal Dock, started separately
//   • a full-screen application launcher (Application Dashboard)
// Applied once on first login by arka-plasma-firstrun via evaluateScript.
// JS strings use single quotes so the whole script can be passed inside a
// double-quoted shell argument.

// --- Top bar (34px) ---------------------------------------------------------
// launcher | spacer | clock (centred) | spacer | tray
var top = new Panel;
top.location = 'top';
top.height = 34;
try { top.floating = false; } catch (e) {}      // flat bar on the screen edge; the dock floats
top.addWidget('org.kde.plasma.kickerdash');     // full-screen launcher button
top.addWidget('org.kde.plasma.panelspacer');
var clock = top.addWidget('org.kde.plasma.digitalclock');
// Readable date beside the time ("Sat 4 Oct 19:33"). Day/month names follow
// the locale; 12/24-hour is left at the locale default (use24hFormat unset).
clock.currentConfigGroup = ['Appearance'];
clock.writeConfig('showDate', true);
clock.writeConfig('dateDisplayFormat', 1);      // beside the time
clock.writeConfig('dateFormat', 'custom');
// The trailing space keeps the date and time apart: on a bar attached to the
// screen edge Plasma's clock otherwise draws them touching ("Oct05:49").
clock.writeConfig('customDateFormat', 'ddd d MMM ');
top.addWidget('org.kde.plasma.panelspacer');
top.addWidget('org.kde.plasma.systemtray');
var topId = top.id;

// --- Dock ---------------------------------------------------------------------
// The dock is Crystal Dock, its own window started at login (see crystal-dock/),
// not a Plasma panel: a panel can't grow icons above itself the way the macOS
// Dock does.

// --- Remove the original default panel(s) -----------------------------------
var all = panels();
for (var i = 0; i < all.length; i++) {
    if (all[i].id != topId) {
        all[i].remove();
    }
}
