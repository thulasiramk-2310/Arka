/*
    The ArkaOS name and tagline for the top bar. The tagline can be hidden or reworded in the
    widget's settings. On a narrow bar, turn the tagline off in the widget settings.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as P5Support

PlasmoidItem {
    id: root
    preferredRepresentation: fullRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    // The name opens the launcher, as the mark beside it does: people click either.
    P5Support.DataSource { id: run; engine: "executable"; onNewData: (s, d) => disconnectSource(s) }

    fullRepresentation: Item {
        readonly property real rowWidth: name.implicitWidth + (Plasmoid.configuration.showTagline ? tagSize.implicitWidth + row.spacing : 0)
        Layout.minimumWidth: rowWidth
        Layout.preferredWidth: rowWidth
        Layout.fillHeight: true
        implicitHeight: row.implicitHeight
        Accessible.role: Accessible.Button
        Accessible.name: "ArkaOS" + (Plasmoid.configuration.showTagline ? ", " + Plasmoid.configuration.tagline : "")

        RowLayout {
            id: row
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            spacing: Kirigami.Units.largeSpacing
            Text { id: name; text: "ArkaOS"; color: Kirigami.Theme.textColor; font.pixelSize: Kirigami.Theme.defaultFont.pixelSize * 1.25; font.weight: Font.DemiBold }
            Text { id: tagSize; visible: false; text: Plasmoid.configuration.tagline; font: Kirigami.Theme.smallFont }
            Text {
                id: tag
                visible: Plasmoid.configuration.showTagline
                Layout.fillWidth: true
                text: Plasmoid.configuration.tagline
                color: Kirigami.Theme.disabledTextColor; font: Kirigami.Theme.smallFont
                elide: Text.ElideRight
            }
        }
        // Covers the whole widget, so a click anywhere on the name opens the launcher.
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: run.connectSource("dbus-send --session --type=method_call --dest=org.kde.plasmashell /PlasmaShell org.kde.PlasmaShell.activateLauncherMenu")
        }
    }
}
