/*
    A round search button. It opens KRunner, which searches what KRunner is set up to search.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root
    preferredRepresentation: fullRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground

    P5Support.DataSource { id: run; engine: "executable"; onNewData: (s, d) => disconnectSource(s) }
    function open() { run.connectSource("qdbus6 org.kde.krunner /App org.kde.krunner.App.display") }

    fullRepresentation: Item {
        id: btn
        readonly property real side: Math.max(Kirigami.Units.gridUnit * 1.6, Math.min(root.height, root.width))
        Layout.minimumWidth: side; Layout.minimumHeight: side
        Layout.preferredWidth: side; Layout.preferredHeight: side
        activeFocusOnTab: true
        Accessible.role: Accessible.Button; Accessible.name: i18n("Search")
        Keys.onReturnPressed: root.open(); Keys.onSpacePressed: root.open()
        Rectangle {
            anchors.centerIn: parent; width: Math.min(parent.width, parent.height) - 4; height: width; radius: width / 2
            color: ma.containsMouse || btn.activeFocus ? Qt.rgba(1, 1, 1, 0.16) : Qt.rgba(0.05, 0.07, 0.12, 0.42)
            border.color: btn.activeFocus ? Kirigami.Theme.highlightColor : Qt.rgba(1, 1, 1, 0.18)
            Kirigami.Icon { anchors.centerIn: parent; width: parent.width * 0.55; height: width; source: "edit-find"; color: Kirigami.Theme.textColor; isMask: true }
        }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.open() }
    }
}
