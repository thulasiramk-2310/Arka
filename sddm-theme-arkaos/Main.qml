import QtQuick
import QtQuick.Shapes
import QtQuick.Effects

// ArkaOS login screen (SDDM, Theme-API 2.0).
//
// Layout: large clock up top, one login card in the centre, a quiet footer
// with session / keyboard layout on the left and power actions on the right.
// Everything is reachable with Tab and shows a 2 px accent focus ring.
// Icons are drawn as vector shapes so they never fall back to tofu boxes.

Rectangle {
    id: root
    width: 1920
    height: 1080
    color: c.bg

    // --- tokens ----------------------------------------------------------------
    // Accent: theme.conf "accent" = auto (follow the desktop) or a #rrggbb.
    Loader { id: probe; source: "AccentProbe.qml" }
    function luminance(col) {
        col = Qt.lighter(col, 1.0)   // accept "#rrggbb" strings as well as colors
        function ch(v) { return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }
        return 0.2126 * ch(col.r) + 0.7152 * ch(col.g) + 0.0722 * ch(col.b)
    }
    function contrast(a, b) {
        var la = luminance(a), lb = luminance(b)
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }
    function readable(col) {
        var out = col
        for (var i = 0; i < 12 && contrast(out, "#0f141c") < 3; i++)
            out = Qt.tint(out, "#33ffffff")   // blend toward white, keep the hue
        return contrast(out, "#0f141c") >= 3 ? out : "#f5f7fa"
    }
    readonly property color pickedAccent: {
        var want = config.accent ? String(config.accent).trim() : "auto"
        if (/^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(want))
            return want
        if (probe.status === Loader.Ready && probe.item && probe.item.fromScheme)
            return probe.item.accent
        return config.fallbackAccent ? config.fallbackAccent : "#16c784"
    }

    QtObject {
        id: c
        readonly property color bg:      "#07080e"
        readonly property color card:    "#e60f141c"
        readonly property color field:   "#161c24"
        readonly property color line:    "#1e2630"
        readonly property color accent:  root.pickedAccent
        readonly property color accentHi: Qt.lighter(accent, 1.12)
        // Text on an accent fill: whichever of near-black / white reads better.
        readonly property color onAccent: root.contrast(accent, "#07080e") >= root.contrast(accent, "#ffffff") ? "#07080e" : "#ffffff"
        // Accent used as a line or text on the dark card: lightened until it
        // reaches 3:1 (WCAG non-text contrast), so dark accents stay visible.
        readonly property color accentFg: root.readable(accent)
        readonly property color ring: accentFg
        readonly property color textHi:  "#f5f7fa"
        readonly property color textLo:  "#9aa4b2"
        readonly property color hint:    "#8d97a6"   // 5.8:1 on field
        readonly property color danger:  "#ff6b6d"   // 6.7:1 on card
        readonly property color warn:    "#e6b450"
        readonly property string font:   "Inter"
    }

    // One scale factor so 1366x768 laptops and 2560x1600 panels both look right.
    readonly property real s: Math.max(0.8, Math.min(1.6, Math.min(width / 1920, height / 1080)))
    function px(v) { return Math.round(v * s) }

    // --- state -----------------------------------------------------------------
    property bool busy: false
    property bool dimmed: false
    property int sessionIndex: sessionModel.lastIndex >= 0 ? sessionModel.lastIndex : 0
    property string errorText: ""

    function wake() {
        dimmed = false
        idleTimer.restart()
    }

    function doLogin() {
        if (busy)
            return
        if (userField.text.length === 0) {
            userField.forceActiveFocus()
            return
        }
        busy = true
        errorText = ""
        sddm.login(userField.text, pwd.text, sessionIndex)
    }

    Timer {
        id: idleTimer
        interval: 60000
        running: true
        onTriggered: if (!root.busy) root.dimmed = true
    }

    Connections {
        target: sddm
        function onLoginFailed() {
            root.busy = false
            root.errorText = "That password didn't work. Try again."
            pwd.text = ""
            pwd.forceActiveFocus()
            shake.restart()
        }
        function onLoginSucceeded() {
            root.errorText = ""
        }
    }

    // --- background ------------------------------------------------------------
    // Blurred so any wallpaper (including one with text) sits quietly behind
    // the card, the way GNOME and macOS treat their lock screens. The plain
    // image stays underneath: on software rendering (no GPU, e.g. VMs) the
    // effect draws nothing and the unblurred wallpaper shows instead of black.
    Image {
        id: bgImage
        anchors.fill: parent
        source: config.background ? config.background : ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        opacity: 0.75
    }
    MultiEffect {
        anchors.fill: parent
        source: bgImage
        opacity: 0.9
        blurEnabled: true
        blur: 0.7
        blurMax: 64
        saturation: 0.15
        autoPaddingEnabled: false
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#b307080e" }
            GradientStop { position: 0.55; color: "#8007080e" }
            GradientStop { position: 1.0; color: "#e607080e" }
        }
    }

    // --- reusable bits ---------------------------------------------------------
    // Focus ring drawn outside the control so it never covers content.
    component FocusRing: Rectangle {
        property Item target
        anchors.fill: target
        anchors.margins: -root.px(4)
        radius: target ? target.radius + root.px(4) : 0
        color: "transparent"
        border.width: 2
        border.color: c.ring
        visible: target ? target.activeFocus : false
    }

    // Round icon button with a label underneath shown on hover/focus.
    component IconButton: Rectangle {
        id: btn
        property string label
        property color hot: c.textHi
        property alias icon: iconSlot.data
        signal activated()

        width: root.px(44); height: width; radius: width / 2
        color: area.containsMouse || activeFocus ? c.field : "transparent"
        border.width: 1
        border.color: c.line
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: label
        Keys.onReturnPressed: activated()
        Keys.onEnterPressed: activated()
        Keys.onSpacePressed: activated()

        Item { id: iconSlot; anchors.centerIn: parent; width: root.px(20); height: width }

        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: btn.activated()
        }
        FocusRing { target: btn }
        Text {
            anchors.top: parent.bottom
            anchors.topMargin: root.px(8)
            anchors.horizontalCenter: parent.horizontalCenter
            text: btn.label
            color: c.textLo
            font.family: c.font
            font.pixelSize: root.px(12)
            opacity: area.containsMouse || btn.activeFocus ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 120 } }
        }
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    // Stroke-only vector icon on a 24x24 grid, scaled to its box.
    component Glyph: Shape {
        id: g
        property color tint: c.textLo
        property real strokeW: 2
        default property alias paths: sp.pathElements
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            id: sp
            strokeColor: g.tint
            strokeWidth: g.strokeW
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            scale: Qt.size(g.width / 24, g.height / 24)
        }
    }

    // Small text pill used for session and keyboard layout pickers.
    component Pill: Rectangle {
        id: pill
        property string label
        property string value
        signal activated()
        height: root.px(34)
        width: pillText.implicitWidth + root.px(28)
        radius: height / 2
        color: pillArea.containsMouse || activeFocus ? c.field : "transparent"
        border.width: 1
        border.color: c.line
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: label + ": " + value
        Keys.onReturnPressed: activated()
        Keys.onSpacePressed: activated()
        Text {
            id: pillText
            anchors.centerIn: parent
            text: pill.value
            color: c.textLo
            font.family: c.font
            font.pixelSize: root.px(13)
        }
        MouseArea {
            id: pillArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: pill.activated()
        }
        FocusRing { target: pill }
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    // Hidden readers for model roles we need by index.
    Item {
        visible: false
        Repeater { id: sessionNames; model: sessionModel; delegate: Item { property string n: model.name } }
        Repeater {
            id: userInfo
            model: userModel
            delegate: Item {
                property string n: model.name
                property string rn: model.realName
                property string ic: model.icon
            }
        }
    }
    function userIcon(name) {
        for (var i = 0; i < userInfo.count; i++) {
            var it = userInfo.itemAt(i)
            if (it && it.n === name) return it.ic
        }
        return ""
    }

    // --- clock -----------------------------------------------------------------
    Column {
        id: clockCol
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.topMargin: root.height * 0.11
        spacing: root.px(4)
        Text {
            id: clock
            anchors.horizontalCenter: parent.horizontalCenter
            color: c.textHi
            font.family: c.font
            font.pixelSize: root.px(84)
            font.weight: Font.Light
            function upd() { text = Qt.formatTime(new Date(), Qt.locale().timeFormat(Locale.ShortFormat).indexOf("AP") >= 0 ? "h:mm" : "hh:mm") }
            Component.onCompleted: upd()
        }
        Text {
            id: dateLine
            anchors.horizontalCenter: parent.horizontalCenter
            color: c.textLo
            font.family: c.font
            font.pixelSize: root.px(18)
            function upd() { text = Qt.formatDate(new Date(), "dddd, d MMMM") }
            Component.onCompleted: upd()
        }
    }
    Timer {
        interval: 1000; running: true; repeat: true
        onTriggered: { clock.upd(); dateLine.upd() }
    }

    // --- login card ------------------------------------------------------------
    Rectangle {
        id: panel
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: clockCol.bottom
        anchors.topMargin: root.px(48)
        width: root.px(380)
        height: col.implicitHeight + root.px(56)
        radius: root.px(22)
        color: c.card
        border.width: 1
        border.color: c.line
        opacity: 0
        transform: Translate { id: shakeX; x: 0 }

        Component.onCompleted: intro.start()
        ParallelAnimation {
            id: intro
            NumberAnimation { target: panel; property: "opacity"; from: 0; to: 1; duration: 450; easing.type: Easing.OutCubic }
            NumberAnimation { target: panel; property: "anchors.topMargin"; from: root.px(64); to: root.px(48); duration: 450; easing.type: Easing.OutCubic }
        }
        SequentialAnimation {
            id: shake
            NumberAnimation { target: shakeX; property: "x"; to: -root.px(10); duration: 50 }
            NumberAnimation { target: shakeX; property: "x"; to: root.px(10); duration: 70 }
            NumberAnimation { target: shakeX; property: "x"; to: -root.px(6); duration: 60 }
            NumberAnimation { target: shakeX; property: "x"; to: root.px(4); duration: 50 }
            NumberAnimation { target: shakeX; property: "x"; to: 0; duration: 40 }
        }

        Column {
            id: col
            anchors.centerIn: parent
            width: parent.width - root.px(56)
            spacing: root.px(14)

            // Avatar: account picture if the user has one, otherwise the initial.
            Rectangle {
                id: avatar
                anchors.horizontalCenter: parent.horizontalCenter
                width: root.px(84); height: width; radius: width / 2
                color: c.field
                border.width: 2
                border.color: c.accentFg
                property string pic: root.userIcon(userField.text)
                Image {
                    id: pic
                    anchors.fill: parent
                    anchors.margins: 3
                    source: avatar.pic.length > 0 ? "file://" + avatar.pic : ""
                    fillMode: Image.PreserveAspectCrop
                    visible: false
                }
                Rectangle {   // circular mask
                    id: picMask
                    anchors.fill: pic
                    radius: width / 2
                    visible: pic.status === Image.Ready
                    clip: true
                    color: "transparent"
                    Image { anchors.fill: parent; source: pic.source; fillMode: Image.PreserveAspectCrop }
                }
                Text {
                    anchors.centerIn: parent
                    visible: pic.status !== Image.Ready
                    text: (userField.text.length > 0 ? userField.text.charAt(0) : "?").toUpperCase()
                    color: c.accentFg
                    font.family: c.font
                    font.pixelSize: root.px(36)
                    font.weight: Font.DemiBold
                }
            }

            // Other accounts (only when there is more than one, max 12 like GNOME).
            Flow {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                spacing: root.px(8)
                visible: userModel.count > 1
                Repeater {
                    model: userModel
                    delegate: Pill {
                        visible: index < 12
                        label: "Switch user"
                        value: model.realName.length > 0 ? model.realName : model.name
                        color: userField.text === model.name ? c.field : "transparent"
                        onActivated: { userField.text = model.name; pwd.text = ""; pwd.forceActiveFocus() }
                    }
                }
            }

            // Username
            Rectangle {
                id: userBox
                width: parent.width; height: root.px(48); radius: root.px(12)
                color: c.field
                border.width: 1
                border.color: userField.activeFocus ? c.accentFg : c.line
                Behavior on border.color { ColorAnimation { duration: 120 } }
                TextInput {
                    id: userField
                    objectName: "userField"
                    anchors.fill: parent
                    anchors.leftMargin: root.px(16); anchors.rightMargin: root.px(16)
                    verticalAlignment: TextInput.AlignVCenter
                    color: c.textHi
                    selectionColor: c.accent
                    selectedTextColor: c.onAccent
                    font.family: c.font
                    font.pixelSize: root.px(15)
                    text: userModel.lastUser
                    enabled: !root.busy
                    focus: userModel.lastUser.length === 0
                    KeyNavigation.tab: pwd
                    Accessible.role: Accessible.EditableText
                    Accessible.name: "Username"
                    onAccepted: pwd.forceActiveFocus()
                    onTextChanged: root.wake()
                }
                Text {
                    anchors.left: parent.left; anchors.leftMargin: root.px(16)
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Username"; color: c.hint
                    font.family: c.font; font.pixelSize: root.px(15)
                    visible: userField.text.length === 0
                }
            }

            // Password + reveal toggle + submit arrow
            Rectangle {
                id: pwdBox
                width: parent.width; height: root.px(48); radius: root.px(12)
                color: c.field
                border.width: 1
                border.color: root.errorText.length > 0 ? c.danger
                            : pwd.activeFocus ? c.accentFg : c.line
                Behavior on border.color { ColorAnimation { duration: 120 } }

                TextInput {
                    id: pwd
                    objectName: "password"
                    property bool reveal: false
                    anchors.left: parent.left; anchors.leftMargin: root.px(16)
                    anchors.right: eye.left; anchors.rightMargin: root.px(6)
                    anchors.verticalCenter: parent.verticalCenter
                    color: c.textHi
                    selectionColor: c.accent
                    selectedTextColor: c.onAccent
                    font.family: c.font
                    font.pixelSize: root.px(15)
                    echoMode: reveal ? TextInput.Normal : TextInput.Password
                    passwordCharacter: "•"
                    enabled: !root.busy
                    focus: userModel.lastUser.length > 0
                    clip: true
                    Accessible.role: Accessible.EditableText
                    Accessible.name: "Password"
                    Accessible.passwordEdit: !reveal
                    onAccepted: root.doLogin()
                    onTextChanged: { root.wake(); if (text.length > 0) root.errorText = "" }
                    Keys.onEscapePressed: text = ""
                }
                Text {
                    anchors.left: parent.left; anchors.leftMargin: root.px(16)
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Password"; color: c.hint
                    font.family: c.font; font.pixelSize: root.px(15)
                    visible: pwd.text.length === 0
                }

                // show / hide password
                Rectangle {
                    id: eye
                    anchors.right: go.left; anchors.rightMargin: root.px(4)
                    anchors.verticalCenter: parent.verticalCenter
                    width: root.px(36); height: width; radius: root.px(8)
                    color: eyeArea.containsMouse || activeFocus ? c.line : "transparent"
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: pwd.reveal ? "Hide password" : "Show password"
                    Keys.onSpacePressed: pwd.reveal = !pwd.reveal
                    Keys.onReturnPressed: pwd.reveal = !pwd.reveal
                    Item {
                        anchors.centerIn: parent; width: root.px(20); height: width
                        Glyph {
                            tint: pwd.reveal ? c.accentFg : c.textLo
                            PathMove { x: 2; y: 12 }
                            PathQuad { x: 22; y: 12; controlX: 12; controlY: 2 }
                            PathQuad { x: 2; y: 12; controlX: 12; controlY: 22 }
                            PathMove { x: 15; y: 12 }
                            PathArc { x: 9; y: 12; radiusX: 3; radiusY: 3 }
                            PathArc { x: 15; y: 12; radiusX: 3; radiusY: 3 }
                        }
                        Glyph {   // slash when hidden
                            visible: !pwd.reveal
                            tint: c.textLo
                            PathMove { x: 4; y: 20 }
                            PathLine { x: 20; y: 4 }
                        }
                    }
                    MouseArea {
                        id: eyeArea; anchors.fill: parent; hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { pwd.reveal = !pwd.reveal; pwd.forceActiveFocus() }
                    }
                    FocusRing { target: eye }
                }

                // submit arrow / busy spinner
                Rectangle {
                    id: go
                    anchors.right: parent.right; anchors.rightMargin: root.px(6)
                    anchors.verticalCenter: parent.verticalCenter
                    width: root.px(36); height: width; radius: root.px(10)
                    color: root.busy ? "transparent" : (goArea.containsMouse ? c.accentHi : c.accent)
                    activeFocusOnTab: !root.busy
                    Accessible.role: Accessible.Button
                    Accessible.name: "Sign in"
                    Keys.onReturnPressed: root.doLogin()
                    Keys.onSpacePressed: root.doLogin()
                    Behavior on color { ColorAnimation { duration: 120 } }
                    Item {
                        anchors.centerIn: parent; width: root.px(18); height: width
                        visible: !root.busy
                        Glyph {
                            tint: c.onAccent; strokeW: 2.4
                            PathMove { x: 4; y: 12 }
                            PathLine { x: 20; y: 12 }
                            PathMove { x: 13; y: 5 }
                            PathLine { x: 20; y: 12 }
                            PathLine { x: 13; y: 19 }
                        }
                    }
                    Item {
                        id: spinner
                        anchors.centerIn: parent; width: root.px(22); height: width
                        visible: root.busy
                        Glyph {
                            tint: c.accentFg; strokeW: 2.6
                            PathMove { x: 12; y: 2 }
                            PathArc { x: 22; y: 12; radiusX: 10; radiusY: 10 }
                        }
                        RotationAnimation on rotation {
                            running: root.busy; from: 0; to: 360; duration: 800; loops: Animation.Infinite
                        }
                    }
                    MouseArea {
                        id: goArea; anchors.fill: parent; hoverEnabled: true
                        enabled: !root.busy
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.doLogin()
                    }
                    FocusRing { target: go }
                }
            }

            // Status line: error, busy, or caps-lock warning — one line, never stacked.
            Text {
                id: status
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                font.family: c.font
                font.pixelSize: root.px(13)
                text: root.errorText.length > 0 ? root.errorText
                    : root.busy ? "Signing in…"
                    : keyboard.capsLock ? "Caps Lock is on"
                    : " "
                color: root.errorText.length > 0 ? c.danger
                     : root.busy ? c.textLo
                     : c.warn
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
        }
    }

    // --- footer ----------------------------------------------------------------
    Row {
        id: leftFooter
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: root.px(32)
        spacing: root.px(10)

        Pill {
            visible: sessionModel.count > 1
            label: "Session"
            value: {
                var it = sessionNames.itemAt(root.sessionIndex)
                return it ? it.n : "Session"
            }
            onActivated: root.sessionIndex = (root.sessionIndex + 1) % sessionModel.count
        }
        Pill {
            visible: keyboard.layouts.length > 1
            label: "Keyboard layout"
            value: {
                var l = keyboard.layouts[keyboard.currentLayout]
                return l ? l.shortName.toUpperCase() : ""
            }
            onActivated: keyboard.currentLayout = (keyboard.currentLayout + 1) % keyboard.layouts.length
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: sddm.hostName
            color: c.textLo
            font.family: c.font
            font.pixelSize: root.px(13)
            leftPadding: root.px(4)
        }
    }

    Row {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: root.px(32)
        anchors.bottomMargin: root.px(52)
        spacing: root.px(16)

        IconButton {
            visible: sddm.canSuspend
            label: "Sleep"
            onActivated: sddm.suspend()
            icon: Glyph {
                PathMove { x: 21; y: 12.79 }
                PathArc { x: 11.21; y: 3; radiusX: 9; radiusY: 9; useLargeArc: true }
                PathArc { x: 21; y: 12.79; radiusX: 7; radiusY: 7; direction: PathArc.Counterclockwise }
            }
        }
        IconButton {
            visible: sddm.canReboot
            label: "Restart"
            onActivated: sddm.reboot()
            icon: Glyph {
                PathMove { x: 20; y: 12 }
                PathArc { x: 7; y: 5; radiusX: 8; radiusY: 8; useLargeArc: true }
                PathMove { x: 7; y: 1.5 }
                PathLine { x: 7; y: 5.5 }
                PathLine { x: 11; y: 5.5 }
            }
        }
        IconButton {
            visible: sddm.canPowerOff
            label: "Shut down"
            onActivated: sddm.powerOff()
            icon: Glyph {
                PathMove { x: 7; y: 6.5 }
                PathArc { x: 17; y: 6.5; radiusX: 8; radiusY: 8; useLargeArc: true; direction: PathArc.Counterclockwise }
                PathMove { x: 12; y: 2.5 }
                PathLine { x: 12; y: 11 }
            }
        }
    }

    // --- idle dim (privacy + panel wear) ---------------------------------------
    Rectangle {
        anchors.fill: parent
        color: "black"
        opacity: root.dimmed ? 0.72 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 600 } }
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
        onPositionChanged: root.wake()
    }
    Keys.onPressed: function (event) { root.wake(); event.accepted = false }
}
