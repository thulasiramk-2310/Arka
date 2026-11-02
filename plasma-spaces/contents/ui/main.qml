/*
    ArkaOS Spaces: the virtual desktops as a hairline ridge. A click on a peak
    switches to that desktop. With a single desktop there is nothing to
    switch, so the widget hides itself.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami
import org.kde.taskmanager as TaskManager

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation
    Plasmoid.status: vdi.numberOfDesktops > 1 ? PlasmaCore.Types.ActiveStatus
                                              : PlasmaCore.Types.HiddenStatus

    // The ArkaOS accent, saved by arka-apply-look. The panel's own theme colours are Breeze's, so
    // they cannot tell green from blue; the saved choice can.
    property string accent: "green"
    P5Support.DataSource {
        engine: "executable"
        connectedSources: ["cat \"$HOME/.config/arka-accent\" 2>/dev/null"]
        interval: 4000
        onNewData: (source, data) => { root.accent = String(data["stdout"]).trim() === "blue" ? "blue" : "green" }
    }

    TaskManager.VirtualDesktopInfo { id: vdi }

    fullRepresentation: Ridge {
        count: vdi.numberOfDesktops
        current: vdi.desktopIds.indexOf(vdi.currentDesktop)
        lineColor: Qt.alpha(Kirigami.Theme.textColor, 0.7)
        markColor: root.accent === "blue" ? "#4f86ff" : "#4db491"
        onActivated: function (index) { vdi.requestActivate(vdi.desktopIds[index]) }
        Layout.minimumWidth: implicitWidth
        Layout.preferredWidth: implicitWidth
    }
}
