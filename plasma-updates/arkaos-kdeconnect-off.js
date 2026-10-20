// ArkaOS: KDE Connect is off by default, because while it runs it makes the
// computer visible to phones and computers on the same network. Its tray
// widget starts the KDE Connect service when the panel loads, so this takes
// the widget out of every tray, once per account. Adding the widget back
// (tray settings) turns KDE Connect on at every login again.
panels().forEach(function (panel) {
    panel.widgets('org.kde.plasma.systemtray').forEach(function (tray) {
        // Plasma 6.7 keeps the tray's entries in the tray itself; older
        // versions in a separate containment named by SystrayContainmentId.
        tray.currentConfigGroup = [];
        var id = tray.readConfig('SystrayContainmentId', '');
        var box = id !== '' ? desktopById(id) : tray;
        if (!box) {
            return;
        }
        // The tray drops an entry that is no longer in extraItems.
        box.currentConfigGroup = ['General'];
        var asList = function (v) { return Array.isArray(v) ? v : String(v || '').split(',').filter(String); };
        box.writeConfig('extraItems', asList(box.readConfig('extraItems', [])).filter(function (entry) {
            return entry != 'org.kde.kdeconnect';
        }));
        var known = asList(box.readConfig('knownItems', []));
        if (known.indexOf('org.kde.kdeconnect') < 0) {
            known.push('org.kde.kdeconnect');
        }
        box.writeConfig('knownItems', known);
    });
});
