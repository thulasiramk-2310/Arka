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

    fullRepresentation: RowLayout {
        spacing: Kirigami.Units.largeSpacing
        Layout.minimumWidth: name.implicitWidth + (Plasmoid.configuration.showTagline ? tagSize.implicitWidth + spacing : 0)
        Layout.preferredWidth: name.implicitWidth + (Plasmoid.configuration.showTagline ? tagSize.implicitWidth + spacing : 0)
        Accessible.role: Accessible.Button
        Accessible.name: "ArkaOS" + (Plasmoid.configuration.showTagline ? ", " + Plasmoid.configuration.tagline : "")
        Text { id: name; text: "ArkaOS"; color: Kirigami.Theme.textColor; font.pixelSize: Kirigami.Theme.defaultFont.pixelSize * 1.25; font.weight: Font.DemiBold }
        Text { id: tagSize; visible: false; text: Plasmoid.configuration.tagline; font: Kirigami.Theme.smallFont }
        Text {
            id: tag
            visible: Plasmoid.configuration.showTagline
            text: Plasmoid.configuration.tagline
            color: Kirigami.Theme.disabledTextColor; font: Kirigami.Theme.smallFont
            elide: Text.ElideRight
        }
        TapHandler {
            onTapped: run.connectSource("dbus-send --session --type=method_call --dest=org.kde.plasmashell /PlasmaShell org.kde.PlasmaShell.activateLauncherMenu")
        }
    }
}
