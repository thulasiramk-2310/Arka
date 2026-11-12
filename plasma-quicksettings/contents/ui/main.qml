/*
    Quick settings (design 2026-11-11): Wi-Fi, Bluetooth and Do Not Disturb tiles, volume and
    brightness, in one panel. It sits next to KDE's own tray icons, which keep their full pop-ups.

    Every system call goes through the same tools ArkaOS's Settings uses and is bounded with `timeout`, so a
    missing or slow device never freezes the bar (a bar once hung on bluetoothctl in a VM). A tile whose
    hardware is missing says so and cannot be pressed; nothing is shown as on unless the tool reported it.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami
import org.kde.notificationmanager as NotificationManager

PlasmoidItem {
    id: root
    preferredRepresentation: compactRepresentation
    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    toolTipMainText: i18n("Quick settings")

    // --- state, filled by refresh() -------------------------------------------------
    property string wifi: "unknown"        // on, off, none, unknown
    property string wifiName: ""
    property string bt: "unknown"          // on, off, none, unknown
    property real volume: -1               // 0..1, -1 = no output
    property bool muted: false
    property int bright: -1                // current, -1 = no control
    property int brightMax: 0
    property bool busy: false

    NotificationManager.Settings { id: notif; live: true }
    readonly property bool dnd: notif.notificationsInhibitedByApplication
        || (notif.notificationsInhibitedUntil !== undefined && !isNaN(notif.notificationsInhibitedUntil)
            && notif.notificationsInhibitedUntil > new Date())

    // --- running commands ---------------------------------------------------------------
    property var pending: ({})
    P5Support.DataSource {
        id: exec
        engine: "executable"
        onNewData: (source, data) => {
            var cb = root.pending[source]
            delete root.pending[source]
            disconnectSource(source)
            if (cb) cb(data["exit code"], String(data["stdout"] || ""))
        }
    }
    function run(cmd, cb) {
        // A unique tail keeps two identical commands from sharing one source.
        var src = cmd + " # " + Date.now() + Math.random()
        pending[src] = cb
        exec.connectSource(src)
    }

    function refresh() {
        run("sh -c 'nmcli -t -f TYPE dev | grep -qx wifi || { echo none; exit 0; }; timeout 3 nmcli -t -f WIFI g; timeout 3 nmcli -t -f ACTIVE,SSID dev wifi | sed -n \"s/^yes://p\" | head -1'",
            function (code, out) {
                var l = out.trim().split("\n")
                if (code !== 0) { root.wifi = "unknown"; return }
                if (l[0] === "none") { root.wifi = "none"; root.wifiName = ""; return }
                root.wifi = l[0] === "enabled" ? "on" : "off"
                root.wifiName = l.length > 1 ? l[1] : ""
            })
        run("sh -c 'timeout 2 bluetoothctl show 2>/dev/null | grep -m1 Powered: || echo none'", function (code, out) {
            var t = out.trim()
            root.bt = t === "none" || t.length === 0 ? "none" : t.indexOf("yes") >= 0 ? "on" : "off"
        })
        run("timeout 2 wpctl get-volume @DEFAULT_AUDIO_SINK@", function (code, out) {
            var m = /Volume:\s*([0-9.]+)/.exec(out)
            root.volume = code === 0 && m ? Math.min(1, Number(m[1])) : -1
            root.muted = out.indexOf("MUTED") >= 0
        })
        run("sh -c 'timeout 2 busctl --user --json=short get-property org.kde.ScreenBrightness /org/kde/ScreenBrightness/display0 org.kde.ScreenBrightness.Display Brightness MaxBrightness'",
            function (code, out) {
                var l = out.trim().split("\n")
                try {
                    root.bright = code === 0 ? JSON.parse(l[0]).data : -1
                    root.brightMax = code === 0 ? JSON.parse(l[1]).data : 0
                } catch (e) { root.bright = -1; root.brightMax = 0 }
            })
    }
    onExpandedChanged: if (expanded) refresh()
    Component.onCompleted: refresh()

    function toggleWifi() {
        if (wifi !== "on" && wifi !== "off") return
        run("timeout 5 nmcli radio wifi " + (wifi === "on" ? "off" : "on"), function () { refresh() })
    }
    function toggleBt() {
        if (bt !== "on" && bt !== "off") return
        run("timeout 5 bluetoothctl power " + (bt === "on" ? "off" : "on"), function () { refresh() })
    }
    function toggleDnd() {
        if (dnd) {
            notif.notificationsInhibitedUntil = undefined
            notif.revokeApplicationInhibitions()
        } else {
            // Until turned off again: a date far enough ahead that it never runs out on its own.
            var d = new Date()
            d.setFullYear(d.getFullYear() + 10)
            notif.notificationsInhibitedUntil = d
        }
        notif.save()
    }
    function launch(cmd) {
        root.expanded = false
        run(cmd, function () {})
    }

    compactRepresentation: Item {
        id: button
        Layout.minimumWidth: Kirigami.Units.iconSizes.medium + Kirigami.Units.smallSpacing * 2
        Layout.preferredWidth: Layout.minimumWidth
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: i18n("Quick settings")
        Keys.onReturnPressed: root.expanded = !root.expanded
        Keys.onSpacePressed: root.expanded = !root.expanded
        Rectangle {
            anchors.centerIn: parent
            width: parent.width; height: Math.min(parent.height - 4, Kirigami.Units.gridUnit * 1.9); radius: height / 2
            color: ma.containsMouse || button.activeFocus || root.expanded ? Qt.rgba(1, 1, 1, 0.14) : "transparent"
            border.color: button.activeFocus ? Kirigami.Theme.highlightColor : "transparent"
        }
        Kirigami.Icon {
            anchors.centerIn: parent
            width: Kirigami.Units.iconSizes.small; height: width
            source: "configure-symbolic"
            color: Kirigami.Theme.textColor
        }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.expanded = !root.expanded }
    }

    component Tile: QQC2.AbstractButton {
        id: tile
        property string iconName
        property string title
        property string state
        property bool on: false
        Layout.fillWidth: true
        Layout.preferredWidth: 1
        implicitHeight: Kirigami.Units.gridUnit * 3.4
        checkable: false
        Accessible.role: Accessible.CheckBox
        Accessible.checkable: true
        Accessible.checked: on
        Accessible.name: title + ": " + state
        Keys.onReturnPressed: clicked()
        Keys.onEnterPressed: clicked()
        background: Rectangle {
            radius: Kirigami.Units.cornerRadius * 2
            color: tile.on ? Qt.rgba(Kirigami.Theme.positiveTextColor.r, Kirigami.Theme.positiveTextColor.g, Kirigami.Theme.positiveTextColor.b, 0.16)
                 : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, tile.hovered ? 0.10 : 0.05)
            border.width: tile.visualFocus ? 2 : 1
            border.color: tile.visualFocus ? Kirigami.Theme.highlightColor
                        : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.12)
        }
        contentItem: RowLayout {
            spacing: Kirigami.Units.largeSpacing
            opacity: tile.enabled ? 1 : 0.55
            Rectangle {
                Layout.leftMargin: Kirigami.Units.smallSpacing * 2
                implicitWidth: Kirigami.Units.iconSizes.medium; implicitHeight: implicitWidth; radius: width / 2
                color: tile.on ? Kirigami.Theme.positiveTextColor
                     : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.10)
                Kirigami.Icon {
                    anchors.centerIn: parent
                    width: Kirigami.Units.iconSizes.small; height: width
                    source: tile.iconName
                    color: tile.on ? Kirigami.Theme.backgroundColor : Kirigami.Theme.textColor
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                PlasmaComponents.Label { Layout.fillWidth: true; text: tile.title; font.weight: Font.DemiBold; elide: Text.ElideRight }
                PlasmaComponents.Label { Layout.fillWidth: true; text: tile.state; font: Kirigami.Theme.smallFont; opacity: 0.8; elide: Text.ElideRight }
            }
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 22
        Layout.minimumWidth: Kirigami.Units.gridUnit * 19
        spacing: Kirigami.Units.largeSpacing

        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: Kirigami.Units.smallSpacing * 2
            rowSpacing: Kirigami.Units.smallSpacing * 2
            Tile {
                iconName: "network-wireless-symbolic"
                title: i18n("Wi-Fi")
                on: root.wifi === "on"
                enabled: root.wifi === "on" || root.wifi === "off"
                state: root.wifi === "none" ? i18n("No Wi-Fi adapter") : root.wifi === "unknown" ? i18n("Unavailable")
                     : root.wifi === "off" ? i18n("Off") : root.wifiName.length > 0 ? root.wifiName : i18n("On, not connected")
                onClicked: root.toggleWifi()
            }
            Tile {
                iconName: "network-bluetooth-symbolic"
                title: i18n("Bluetooth")
                on: root.bt === "on"
                enabled: root.bt === "on" || root.bt === "off"
                state: root.bt === "none" ? i18n("No Bluetooth adapter") : root.bt === "unknown" ? i18n("Unavailable")
                     : root.bt === "on" ? i18n("On") : i18n("Off")
                onClicked: root.toggleBt()
            }
            Tile {
                iconName: "notifications-disabled-symbolic"
                title: i18n("Do Not Disturb")
                on: root.dnd
                state: root.dnd ? i18n("On, until you turn it off") : i18n("Off")
                onClicked: root.toggleDnd()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon { source: root.muted ? "audio-volume-muted-symbolic" : "audio-volume-high-symbolic"; implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: implicitWidth }
            PlasmaComponents.Label { text: i18n("Volume"); Layout.preferredWidth: Kirigami.Units.gridUnit * 5 }
            PlasmaComponents.Slider {
                id: vol
                Layout.fillWidth: true
                from: 0; to: 100; stepSize: 1
                enabled: root.volume >= 0
                value: root.volume >= 0 ? Math.round(root.volume * 100) : 0
                Accessible.name: i18n("Volume")
                onMoved: root.run("timeout 2 wpctl set-volume @DEFAULT_AUDIO_SINK@ " + (value / 100).toFixed(2), function () {})
            }
            PlasmaComponents.Label { visible: root.volume < 0; text: i18n("No sound output"); font: Kirigami.Theme.smallFont; opacity: 0.8 }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon { source: "brightness-high-symbolic"; implicitWidth: Kirigami.Units.iconSizes.small; implicitHeight: implicitWidth }
            PlasmaComponents.Label { text: i18n("Brightness"); Layout.preferredWidth: Kirigami.Units.gridUnit * 5 }
            PlasmaComponents.Slider {
                Layout.fillWidth: true
                from: 0; to: Math.max(1, root.brightMax); stepSize: Math.max(1, Math.round(root.brightMax / 100))
                enabled: root.bright >= 0 && root.brightMax > 0
                value: Math.max(0, root.bright)
                Accessible.name: i18n("Brightness")
                onMoved: root.run("timeout 2 busctl --user call org.kde.ScreenBrightness /org/kde/ScreenBrightness/display0 org.kde.ScreenBrightness.Display SetBrightness iu " + Math.round(value) + " 0", function () {})
            }
            PlasmaComponents.Label { visible: !(root.bright >= 0 && root.brightMax > 0); text: i18n("Not adjustable here"); font: Kirigami.Theme.smallFont; opacity: 0.8 }
        }

        RowLayout {   // the full lists live in ArkaOS's own Wi-Fi and Bluetooth windows
            Layout.fillWidth: true
            PlasmaComponents.ToolButton {
                Layout.fillWidth: true
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                enabled: root.wifi === "on" || root.wifi === "off"
                text: i18n("Wi-Fi networks…")
                icon.name: "network-wireless-symbolic"
                onClicked: root.launch("arka-wifi")
            }
            PlasmaComponents.ToolButton {
                Layout.fillWidth: true
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                enabled: root.bt === "on" || root.bt === "off"
                text: i18n("Bluetooth devices…")
                icon.name: "network-bluetooth-symbolic"
                onClicked: root.launch("arka-bluetooth")
            }
        }

        Kirigami.Separator { Layout.fillWidth: true }
        RowLayout {
            Layout.fillWidth: true
            PlasmaComponents.ToolButton {
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                text: i18n("All settings")
                icon.name: "configure-symbolic"
                onClicked: root.launch("arka-settings-gtk")
            }
            Item { Layout.fillWidth: true }
            PlasmaComponents.ToolButton {
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                icon.name: "system-lock-screen-symbolic"
                Accessible.name: i18n("Lock screen")
                PlasmaComponents.ToolTip { text: i18n("Lock screen") }
                onClicked: root.launch("loginctl lock-session")
            }
            PlasmaComponents.ToolButton {
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                icon.name: "system-shutdown-symbolic"
                Accessible.name: i18n("Shut down, restart or sign out")
                PlasmaComponents.ToolTip { text: i18n("Shut down, restart or sign out") }
                onClicked: root.launch("busctl --user call org.kde.LogoutPrompt /LogoutPrompt org.kde.LogoutPrompt promptAll")
            }
        }
    }
}
