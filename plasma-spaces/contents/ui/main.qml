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
import org.kde.kirigami as Kirigami
import org.kde.taskmanager as TaskManager

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation
    Plasmoid.status: vdi.numberOfDesktops > 1 ? PlasmaCore.Types.ActiveStatus
                                              : PlasmaCore.Types.HiddenStatus

    TaskManager.VirtualDesktopInfo { id: vdi }

    fullRepresentation: Ridge {
        count: vdi.numberOfDesktops
        current: vdi.desktopIds.indexOf(vdi.currentDesktop)
        lineColor: Qt.alpha(Kirigami.Theme.textColor, 0.7)
        markColor: Kirigami.Theme.highlightColor
        onActivated: function (index) { vdi.requestActivate(vdi.desktopIds[index]) }
        Layout.minimumWidth: implicitWidth
        Layout.preferredWidth: implicitWidth
    }
}
