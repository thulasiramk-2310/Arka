// ArkaOS desktop layout for KDE Plasma — evokes the ArkaOS design:
//   • a slim top bar (launcher + clock + system tray)
//   • a floating, icons-only dock at the bottom
//   • a full-screen application launcher (Application Dashboard)
// Applied once on first login by arka-plasma-firstrun via evaluateScript.
// JS strings use single quotes so the whole script can be passed inside a
// double-quoted shell argument.

// --- Top bar (36px) ---------------------------------------------------------
var top = new Panel;
top.location = 'top';
top.height = 36;
top.addWidget('org.kde.plasma.kickerdash');     // full-screen launcher button
top.addWidget('org.kde.plasma.panelspacer');    // push the rest to the right
top.addWidget('org.kde.plasma.systemtray');
top.addWidget('org.kde.plasma.digitalclock');
var topId = top.id;

// --- Floating dock (icons-only task manager) --------------------------------
var dock = new Panel;
dock.location = 'bottom';
dock.height = 48;
try { dock.floating = true; } catch (e) {}
try { dock.lengthMode = 'fit'; } catch (e) {}
try { dock.alignment = 'center'; } catch (e) {}
dock.addWidget('org.kde.plasma.icontasks');
var dockId = dock.id;

// --- Remove the original default panel(s) -----------------------------------
var all = panels();
for (var i = 0; i < all.length; i++) {
    if (all[i].id != topId && all[i].id != dockId) {
        all[i].remove();
    }
}
