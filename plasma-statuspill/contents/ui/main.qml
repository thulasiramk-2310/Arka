/*
    The status pill: how many of arkad's six privacy settings it last confirmed. Green only when all six are
    confirmed and the check is fresh (under 90 seconds old). Amber when some are not. Grey when arkad cannot be
    read or its check is stale. It never says a system is secure or verified, only what was checked.

    Clicking the pill opens a small panel with the same count, one segment and one row per setting, and a
    button to the Privacy Dashboard (design 2026-11-11).

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami
import "Rows.js" as R

PlasmoidItem {
    id: root
    preferredRepresentation: compactRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    toolTipMainText: label

    readonly property var props: ["LastCheck"].concat(R.ROWS.map(function (r) { return r.prop }))
    readonly property string readCmd: "busctl " + Plasmoid.configuration.busFlag + " --json=short get-property org.arka.arkad /org/arka/arkad org.arka.arkad " + props.join(" ")
    property var values: null
    property double nowSecs: Date.now() / 1000
    readonly property double checkedAt: values ? Number(values.LastCheck) : 0
    readonly property bool stale: !values || checkedAt <= 0 || (nowSecs - checkedAt) > R.STALE_SECS
    // The state of each row as shown: nothing counts as in place when the check is missing or stale.
    readonly property var states: R.ROWS.map(function (r) {
        if (!values) return R.UNAVAILABLE
        if (stale) return "Not recently checked"
        return R.stateOf(r.kind, values[r.prop])
    })
    readonly property int configured: states.filter(function (s) { return s === R.CONFIGURED }).length
    readonly property int total: R.ROWS.length
    // "ok", "partial" or "none"
    readonly property string level: stale ? "none" : configured === total ? "ok" : "partial"
    readonly property string label: !values ? i18n("Privacy status unavailable")
                                 : stale ? i18n("Privacy check is out of date")
                                 : configured === total ? i18n("Privacy: %1 of %2 settings in place", configured, total)
                                 : i18n("Privacy: %1 of %2 settings in place, open to see which", configured, total)
    readonly property string heading: !values ? i18n("Can't check right now")
                                   : stale ? i18n("Not recently checked")
                                   : configured === total ? i18n("%1 of %2 settings in place", configured, total)
                                   : i18n("%1 of %2 in place", configured, total)
    readonly property string note: !values ? i18n("The privacy service isn't answering. Nothing is shown as in place until it does.")
                                : i18n("Checked %1. Covers only what is listed here.", R.ago(nowSecs - checkedAt))
    function colorFor(state) {
        return state === R.CONFIGURED ? Kirigami.Theme.positiveTextColor
             : state === R.ATTENTION || state === R.NOT_SET ? Kirigami.Theme.neutralTextColor
             : Kirigami.Theme.disabledTextColor
    }
    readonly property color dot: level === "ok" ? Kirigami.Theme.positiveTextColor : level === "partial" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.disabledTextColor

    P5Support.DataSource {
        engine: "executable"; connectedSources: [root.readCmd]; interval: 30000
        onNewData: (source, data) => { root.values = data["exit code"] === 0 ? R.parse(data["stdout"], root.props) : null; root.nowSecs = Date.now() / 1000 }
    }
    P5Support.DataSource { id: launcher; engine: "executable"; onNewData: (s, d) => disconnectSource(s) }
    Timer { interval: 10000; running: true; repeat: true; onTriggered: root.nowSecs = Date.now() / 1000 }

    function openDashboard() {
        root.expanded = false
        launcher.connectSource("arka-dashboard")
    }

    compactRepresentation: Item {
        id: pill
        readonly property real h: Math.max(Kirigami.Units.gridUnit * 1.6, root.height)
        Layout.minimumHeight: h; Layout.preferredHeight: h
        Layout.minimumWidth: txt.implicitWidth + Kirigami.Units.gridUnit * 2.4 + Kirigami.Units.smallSpacing
        Layout.preferredWidth: Layout.minimumWidth
        activeFocusOnTab: true
        Accessible.role: Accessible.Button; Accessible.name: root.label
        Keys.onReturnPressed: root.expanded = !root.expanded
        Keys.onSpacePressed: root.expanded = !root.expanded
        Rectangle {
            anchors.centerIn: parent; width: parent.width; height: Math.min(parent.height - 4, Kirigami.Units.gridUnit * 1.9); radius: height / 2
            color: ma.containsMouse || pill.activeFocus || root.expanded ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(0.05, 0.07, 0.12, 0.42)
            border.color: pill.activeFocus ? Kirigami.Theme.highlightColor : Qt.rgba(1, 1, 1, 0.18)
            Row {
                anchors.centerIn: parent; spacing: Kirigami.Units.smallSpacing * 2
                Rectangle { width: Kirigami.Units.smallSpacing * 2.2; height: width; radius: width / 2; color: root.dot; anchors.verticalCenter: parent.verticalCenter }
                Text { id: txt; text: root.label; color: Kirigami.Theme.textColor; font: Kirigami.Theme.defaultFont; anchors.verticalCenter: parent.verticalCenter }
            }
        }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.expanded = !root.expanded }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 22
        Layout.minimumWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.largeSpacing

        Text {
            Layout.fillWidth: true
            text: root.heading
            color: Kirigami.Theme.textColor
            font.family: "Noto Serif"
            font.pixelSize: Kirigami.Units.gridUnit * 1.4
            wrapMode: Text.WordWrap
            Accessible.role: Accessible.Heading
        }
        RowLayout {   // one segment per setting: a count, never a score
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Repeater {
                model: root.states
                delegate: Rectangle {
                    required property string modelData
                    Layout.fillWidth: true
                    implicitHeight: Math.round(Kirigami.Units.smallSpacing * 1.5)
                    radius: height / 2
                    color: modelData === R.CONFIGURED ? Kirigami.Theme.positiveTextColor
                         : modelData === R.ATTENTION || modelData === R.NOT_SET ? Kirigami.Theme.neutralTextColor
                         : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.15)
                }
            }
        }
        Text {
            Layout.fillWidth: true
            text: root.note
            color: Kirigami.Theme.disabledTextColor
            font: Kirigami.Theme.smallFont
            wrapMode: Text.WordWrap
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            Repeater {
                model: R.ROWS.length
                delegate: ColumnLayout {
                    required property int index
                    Layout.fillWidth: true
                    spacing: 0
                    Kirigami.Separator { Layout.fillWidth: true }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: Kirigami.Units.smallSpacing * 2
                        Layout.bottomMargin: Kirigami.Units.smallSpacing * 2
                        Accessible.role: Accessible.StaticText
                        Accessible.name: R.ROWS[index].name + ": " + root.states[index]
                        Text {
                            Layout.fillWidth: true
                            text: R.ROWS[index].name
                            color: Kirigami.Theme.textColor
                            font: Kirigami.Theme.defaultFont
                            elide: Text.ElideRight
                        }
                        Kirigami.Icon {
                            visible: root.states[index] === R.CONFIGURED
                            source: "checkmark-symbolic"
                            color: Kirigami.Theme.positiveTextColor
                            implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: implicitWidth
                        }
                        Text {
                            text: root.states[index]
                            color: root.states[index] === R.CONFIGURED ? Kirigami.Theme.textColor : root.colorFor(root.states[index])
                            font: Kirigami.Theme.smallFont
                        }
                    }
                }
            }
            Kirigami.Separator { Layout.fillWidth: true }
        }
        PlasmaComponents.Button {
            Layout.fillWidth: true
            Keys.onReturnPressed: clicked()
            Keys.onEnterPressed: clicked()
            text: i18n("Open Privacy Dashboard")
            icon.name: "security-high-symbolic"
            onClicked: root.openDashboard()
        }
    }
}
