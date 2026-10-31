/*
    The status pill: how many of arkad's six privacy settings it last confirmed. Green only when all six are
    confirmed and the check is fresh (under 90 seconds old). Amber when some are not. Grey when arkad cannot be
    read or its check is stale. It never says a system is secure or verified, only what was checked.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami
import "Rows.js" as R

PlasmoidItem {
    id: root
    preferredRepresentation: fullRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground

    readonly property var props: ["LastCheck"].concat(R.ROWS.map(function (r) { return r.prop }))
    readonly property string readCmd: "busctl " + Plasmoid.configuration.busFlag + " --json=short get-property org.arka.arkad /org/arka/arkad org.arka.arkad " + props.join(" ")
    property var values: null
    property double nowSecs: Date.now() / 1000
    readonly property double checkedAt: values ? Number(values.LastCheck) : 0
    readonly property bool stale: !values || checkedAt <= 0 || (nowSecs - checkedAt) > R.STALE_SECS
    readonly property int configured: {
        var n = 0
        if (!values || stale) return 0
        for (var i = 0; i < R.ROWS.length; i++) if (R.stateOf(R.ROWS[i].kind, values[R.ROWS[i].prop]) === R.CONFIGURED) n++
        return n
    }
    readonly property int total: R.ROWS.length
    // "ok", "partial" or "none"
    readonly property string level: stale ? "none" : configured === total ? "ok" : "partial"
    readonly property string label: !values ? i18n("Privacy status unavailable")
                                 : stale ? i18n("Privacy check is out of date")
                                 : configured === total ? i18n("%1 of %2 settings configured", configured, total)
                                 : i18n("%1 of %2 settings configured, check Privacy", configured, total)
    readonly property color dot: level === "ok" ? Kirigami.Theme.positiveTextColor : level === "partial" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.disabledTextColor

    P5Support.DataSource {
        engine: "executable"; connectedSources: [root.readCmd]; interval: 30000
        onNewData: (source, data) => { root.values = data["exit code"] === 0 ? R.parse(data["stdout"], root.props) : null; root.nowSecs = Date.now() / 1000 }
    }
    P5Support.DataSource { id: launcher; engine: "executable"; onNewData: (s, d) => disconnectSource(s) }
    Timer { interval: 10000; running: true; repeat: true; onTriggered: root.nowSecs = Date.now() / 1000 }

    fullRepresentation: Item {
        id: pill
        readonly property real h: Math.max(Kirigami.Units.gridUnit * 1.6, root.height)
        Layout.minimumHeight: h; Layout.preferredHeight: h
        Layout.minimumWidth: txt.implicitWidth + Kirigami.Units.gridUnit * 2.4 + Kirigami.Units.smallSpacing
        Layout.preferredWidth: Layout.minimumWidth
        activeFocusOnTab: true
        Accessible.role: Accessible.Button; Accessible.name: root.label
        Keys.onReturnPressed: launcher.connectSource("arka-dashboard"); Keys.onSpacePressed: launcher.connectSource("arka-dashboard")
        Rectangle {
            anchors.centerIn: parent; width: parent.width; height: Math.min(parent.height - 4, Kirigami.Units.gridUnit * 1.9); radius: height / 2
            color: ma.containsMouse || pill.activeFocus ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(0.05, 0.07, 0.12, 0.42)
            border.color: pill.activeFocus ? Kirigami.Theme.highlightColor : Qt.rgba(1, 1, 1, 0.18)
            Row {
                anchors.centerIn: parent; spacing: Kirigami.Units.smallSpacing * 2
                Rectangle { width: Kirigami.Units.smallSpacing * 2.2; height: width; radius: width / 2; color: root.dot; anchors.verticalCenter: parent.verticalCenter }
                Text { id: txt; text: root.label; color: Kirigami.Theme.textColor; font: Kirigami.Theme.defaultFont; anchors.verticalCenter: parent.verticalCenter }
            }
        }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: launcher.connectSource("arka-dashboard") }
    }
}
