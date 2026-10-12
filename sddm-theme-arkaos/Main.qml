import QtQuick
import QtQuick.Shapes
import QtQuick.Effects

// ArkaOS login screen (SDDM, Theme-API 2.0).
//
// Two stages, like the lock and sign-in screens of macOS, Windows and GNOME:
//   lock    the wallpaper is the hero: large clock, nothing else in the way.
//           Any key or click moves on; a typed character is kept as the
//           first character of the password, so nothing typed is lost.
//   signin  the wallpaper blurs (focus on the task), the clock slides up,
//           and the account picture, name and a glass password field appear.
// Every control is reachable with Tab and shows a 2 px accent focus ring.
// Icons are vector shapes so they never fall back to tofu boxes.

Rectangle {
    id: root
    width: 1920
    height: 1080
    color: c.bg

    // --- accent: theme.conf "accent" = auto (follow the desktop) or #rrggbb ----
    Loader { id: probe; source: "AccentProbe.qml" }
    Loader { id: battery; source: "BatteryProbe.qml" }

    function luminance(col) {
        col = Qt.lighter(col, 1.0)   // accept "#rrggbb" strings as well as colors
        function ch(v) { return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }
        return 0.2126 * ch(col.r) + 0.7152 * ch(col.g) + 0.0722 * ch(col.b)
    }
    function contrast(a, b) {
        var la = luminance(a), lb = luminance(b)
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }
    // theme.conf "look" = dark (default) or light. arkaos-oobe-apply selects
    // the arkaos-light copy of this theme when the user picks Light in setup.
    readonly property bool light: config.look ? String(config.look).trim() === "light" : false

    // Shift toward white (dark look) or black (light look), keeping the hue,
    // until the colour reads at 3:1 on the UI behind it.
    function readable(col) {
        var behind = light ? "#ffffff" : "#0f141c"
        var toward = light ? "#33000000" : "#33ffffff"
        var out = col
        for (var i = 0; i < 12 && contrast(out, behind) < 3; i++)
            out = Qt.tint(out, toward)
        return contrast(out, behind) >= 3 ? out : (light ? "#111111" : "#f5f7fa")
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
        readonly property color bg:        root.light ? "#eceff3" : "#07080e"
        readonly property color accent:    root.pickedAccent
        readonly property color accentHi:  root.light ? Qt.darker(accent, 1.08) : Qt.lighter(accent, 1.12)
        readonly property color onAccent:  root.contrast(accent, "#07080e") >= root.contrast(accent, "#ffffff") ? "#07080e" : "#ffffff"
        readonly property color accentFg:  root.readable(accent)
        readonly property color ring:      accentFg
        // "glass" = translucent fields/pills: white on the dark look, ink on the light one
        readonly property color glass:     root.light ? Qt.rgba(1, 1, 1, 0.72) : Qt.rgba(1, 1, 1, 0.10)
        readonly property color glassHi:   root.light ? Qt.rgba(1, 1, 1, 0.92) : Qt.rgba(1, 1, 1, 0.16)
        readonly property color glassLine: root.light ? Qt.rgba(0, 0, 0, 0.16) : Qt.rgba(1, 1, 1, 0.22)
        readonly property color textHi:    root.light ? "#111111" : "#f5f7fa"
        readonly property color textLo:    root.light ? "#3d4450" : "#c9d0da"
        readonly property color hint:      root.light ? "#5c636e" : "#b9c1cc"
        readonly property color danger:    root.light ? "#b42318" : "#ff8a8c"
        readonly property color warn:      root.light ? "#b45309" : "#f2c766"
        readonly property color scrim:     root.light ? "#ffffff" : "#000000"
        readonly property string font:     "Inter"
    }

    // One scale factor so 1366x768 laptops and 2560x1600 panels both look right.
    readonly property real s: Math.max(0.75, Math.min(1.6, Math.min(width / 1920, height / 1080)))
    function px(v) { return Math.round(v * s) }

    // --- state -----------------------------------------------------------------
    property string stage: "lock"
    readonly property bool signin: stage === "signin"
    property bool busy: false
    property bool dimmed: false
    property bool askName: userModel.lastUser.length === 0
    property int sessionIndex: sessionModel.lastIndex >= 0 ? sessionModel.lastIndex : 0
    property string errorText: ""

    function wake() {
        dimmed = false
        idleTimer.restart()
    }
    function toSignin(firstChar) {
        wake()
        stage = "signin"
        if (askName) {
            userField.forceActiveFocus()
            if (firstChar.length > 0) userField.insert(userField.cursorPosition, firstChar)
        } else {
            pwd.forceActiveFocus()
            if (firstChar.length > 0) pwd.insert(pwd.cursorPosition, firstChar)
        }
    }
    function toLock() {
        if (busy) return
        pwd.text = ""
        errorText = ""
        stage = "lock"
        lockKeys.forceActiveFocus()
    }
    function doLogin() {
        if (busy) return
        if (userField.text.length === 0) {
            askName = true
            userField.forceActiveFocus()
            return
        }
        busy = true
        errorText = ""
        sddm.login(userField.text, pwd.text, sessionIndex)
    }

    // Untouched sign-in stage (nothing typed) returns to the lock stage after
    // 30 s; after that, dim (privacy, panel wear). Any input wakes it.
    Timer {
        id: idleTimer
        interval: 30000
        running: true
        onTriggered: {
            if (root.busy) return
            if (root.signin && pwd.text.length === 0) { root.toLock(); restart() }
            else root.dimmed = true
        }
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
        function onLoginSucceeded() { root.errorText = "" }
    }

    // --- background --------------------------------------------------------------
    // Sharp on the lock stage, blurred on sign-in (focus on the task, as on
    // Windows and GNOME). The plain image stays underneath: with software
    // rendering (no GPU, e.g. VMs) the blur draws nothing and the darker
    // sign-in scrim keeps the text readable instead.
    Image {
        id: bgImage
        anchors.fill: parent
        source: config.background ? config.background : ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }
    MultiEffect {
        anchors.fill: parent
        source: bgImage
        blurEnabled: true
        blur: 1.0
        blurMax: 96
        blurMultiplier: 0.6
        saturation: 0.1
        brightness: root.light ? 0.06 : -0.08
        autoPaddingEnabled: false
        opacity: root.signin ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 380; easing.type: Easing.OutCubic } }
    }
    // Software rendering can't blur, so the sign-in scrim goes much darker.
    readonly property bool softwareGfx: GraphicsInfo.api === GraphicsInfo.Software
    Rectangle {   // overall scrim
        anchors.fill: parent
        color: c.scrim
        opacity: root.signin ? (root.softwareGfx ? 0.86 : 0.52) : 0.18
        Behavior on opacity { NumberAnimation { duration: 380 } }
    }
    Rectangle {   // top fade so the clock reads on any wallpaper
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: parent.height * 0.5
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.rgba(c.scrim.r, c.scrim.g, c.scrim.b, 0.6) }
            GradientStop { position: 1.0; color: Qt.rgba(c.scrim.r, c.scrim.g, c.scrim.b, 0) }
        }
    }
    Rectangle {   // bottom fade for the footer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: parent.height * 0.28
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.rgba(c.scrim.r, c.scrim.g, c.scrim.b, 0) }
            GradientStop { position: 1.0; color: Qt.rgba(c.scrim.r, c.scrim.g, c.scrim.b, 0.55) }
        }
    }

    // --- reusable bits ---------------------------------------------------------
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

    component IconButton: Rectangle {
        id: btn
        property string label
        property alias icon: iconSlot.data
        signal activated()
        width: root.px(46); height: width; radius: width / 2
        color: area.containsMouse || activeFocus ? c.glassHi : c.glass
        border.width: 1
        border.color: c.glassLine
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
            anchors.bottom: parent.top
            anchors.bottomMargin: root.px(8)
            anchors.horizontalCenter: parent.horizontalCenter
            text: btn.label
            color: c.textHi
            font.family: c.font
            font.pixelSize: root.px(12)
            opacity: area.containsMouse || btn.activeFocus ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 120 } }
        }
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    component Pill: Rectangle {
        id: pill
        property string label
        property string value
        property bool selected: false
        signal activated()
        height: root.px(32)
        width: pillText.implicitWidth + root.px(26)
        radius: height / 2
        color: selected || pillArea.containsMouse || activeFocus ? c.glassHi : c.glass
        border.width: 1
        border.color: selected ? c.accentFg : c.glassLine
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: label + ": " + value
        Keys.onReturnPressed: activated()
        Keys.onSpacePressed: activated()
        Text {
            id: pillText
            anchors.centerIn: parent
            text: pill.value
            color: c.textHi
            font.family: c.font
            font.pixelSize: root.px(13)
            font.weight: Font.Medium
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

    // Arka mark: the boot splash's nested triangles, so boot and login connect.
    component ArkaMark: Shape {
        id: mk
        property color tint: c.accentFg
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: mk.tint; strokeWidth: 1.6; fillColor: "transparent"
            joinStyle: ShapePath.RoundJoin
            scale: Qt.size(mk.width / 24, mk.height / 24)
            PathMove { x: 12; y: 6 }
            PathLine { x: 18; y: 17 }
            PathLine { x: 6; y: 17 }
            PathLine { x: 12; y: 6 }
        }
        ShapePath {
            strokeColor: Qt.rgba(mk.tint.r, mk.tint.g, mk.tint.b, 0.45); strokeWidth: 1.2; fillColor: "transparent"
            joinStyle: ShapePath.RoundJoin
            scale: Qt.size(mk.width / 24, mk.height / 24)
            PathMove { x: 12; y: 2 }
            PathLine { x: 22; y: 20.5 }
            PathLine { x: 2; y: 20.5 }
            PathLine { x: 12; y: 2 }
        }
    }

    // Hidden readers for model roles we need by name.
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
    function userRole(name, key) {
        for (var i = 0; i < userInfo.count; i++) {
            var it = userInfo.itemAt(i)
            if (it && it.n === name) return it[key]
        }
        return ""
    }
    readonly property string displayName: {
        var rn = userRole(userField.text, "rn")
        return rn.length > 0 ? rn : userField.text
    }

    // --- top bar: brand left, status right ---------------------------------------
    Row {
        anchors { left: parent.left; top: parent.top; margins: root.px(28) }
        spacing: root.px(10)
        ArkaMark { width: root.px(22); height: width; anchors.verticalCenter: parent.verticalCenter }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "ArkaOS"
            color: c.textHi
            font.family: c.font
            font.pixelSize: root.px(15)
            font.weight: Font.DemiBold
        }
    }

    Row {
        anchors { right: parent.right; top: parent.top; margins: root.px(28) }
        spacing: root.px(18)

        Text {   // keyboard layout, only when there is a choice
            anchors.verticalCenter: parent.verticalCenter
            visible: keyboard.layouts.length > 1
            text: {
                var l = keyboard.layouts[keyboard.currentLayout]
                return l ? l.shortName.toUpperCase() : ""
            }
            color: c.textHi
            font.family: c.font
            font.pixelSize: root.px(13)
            font.weight: Font.DemiBold
        }

        Row {   // battery: percentage + outline + level fill
            id: batt
            readonly property var b: battery.status === Loader.Ready ? battery.item : null
            visible: b !== null && b.present
            anchors.verticalCenter: parent.verticalCenter
            spacing: root.px(7)
            Accessible.role: Accessible.StaticText
            Accessible.name: b ? "Battery " + b.percent + " percent" + (b.charging ? ", charging" : "") : ""
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: batt.b ? batt.b.percent + "%" : ""
                color: c.textHi
                font.family: c.font
                font.pixelSize: root.px(13)
                font.weight: Font.Medium
            }
            Item {
                width: root.px(26); height: root.px(13)
                anchors.verticalCenter: parent.verticalCenter
                Rectangle {
                    id: shell
                    width: parent.width - root.px(3); height: parent.height
                    radius: root.px(3.5)
                    color: "transparent"
                    border.width: Math.max(1, root.px(1.4))
                    border.color: c.textHi
                    Rectangle {
                        x: root.px(2.4); y: root.px(2.4)
                        height: parent.height - root.px(4.8)
                        width: Math.max(root.px(2), (parent.width - root.px(4.8)) * (batt.b ? batt.b.percent : 0) / 100)
                        radius: root.px(1.5)
                        color: batt.b && batt.b.percent <= 15 && !batt.b.charging ? c.danger
                             : batt.b && batt.b.charging ? c.accentFg : c.textHi
                    }
                }
                Rectangle {
                    anchors.left: shell.right; anchors.leftMargin: root.px(1)
                    anchors.verticalCenter: shell.verticalCenter
                    width: root.px(2); height: root.px(5); radius: root.px(1)
                    color: c.textHi
                }
            }
        }
    }

    // --- clock: big on lock, compact on sign-in ---------------------------------
    Column {
        id: clockCol
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.signin ? root.height * 0.075 : root.height * 0.15
        scale: root.signin ? 0.52 : 1
        transformOrigin: Item.Top
        spacing: root.px(2)
        Behavior on y { NumberAnimation { duration: 420; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: 420; easing.type: Easing.OutCubic } }

        Text {
            id: dateLine
            anchors.horizontalCenter: parent.horizontalCenter
            color: c.textHi
            opacity: 0.92
            font.family: c.font
            font.pixelSize: root.px(24)
            font.weight: Font.Medium
            function upd() { text = Qt.formatDate(new Date(), "dddd, d MMMM") }
            Component.onCompleted: upd()
        }
        Text {
            id: clock
            anchors.horizontalCenter: parent.horizontalCenter
            color: c.textHi
            font.family: c.font
            font.pixelSize: root.px(168)
            font.weight: Font.DemiBold
            font.letterSpacing: -root.px(6)
            function upd() {
                var twelve = Qt.locale().timeFormat(Locale.ShortFormat).indexOf("AP") >= 0
                text = Qt.formatTime(new Date(), twelve ? "h:mm" : "HH:mm")
            }
            Component.onCompleted: upd()
        }
    }
    Timer {
        interval: 1000; running: true; repeat: true
        onTriggered: { clock.upd(); dateLine.upd() }
    }

    // --- lock stage: key/click catcher + hint ------------------------------------
    Item {
        id: lockKeys
        anchors.fill: parent
        focus: true
        enabled: !root.signin
        Accessible.role: Accessible.Button
        Accessible.name: "Press any key or click to sign in"
        Keys.onPressed: function (event) {
            if (root.signin) return
            var t = event.text
            var printable = t.length === 1 && t.charCodeAt(0) >= 32 && t.charCodeAt(0) !== 127
            root.toSignin(printable ? t : "")
            event.accepted = true
        }
        MouseArea {
            anchors.fill: parent
            enabled: !root.signin
            hoverEnabled: true
            onClicked: root.toSignin("")
            onPositionChanged: root.wake()
        }
    }

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.height * 0.09
        spacing: root.px(12)
        opacity: root.signin ? 0 : 1
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 260 } }

        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: root.px(22); height: root.px(22)
            Glyph {   // chevron up, breathing
                tint: c.textHi
                strokeW: 2.2
                PathMove { x: 6; y: 15 }
                PathLine { x: 12; y: 9 }
                PathLine { x: 18; y: 15 }
            }
            SequentialAnimation on opacity {
                running: !root.signin
                loops: Animation.Infinite
                NumberAnimation { from: 0.35; to: 1; duration: 900; easing.type: Easing.InOutSine }
                NumberAnimation { from: 1; to: 0.35; duration: 900; easing.type: Easing.InOutSine }
            }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "Press any key or click to sign in"
            color: c.textHi
            opacity: 0.85
            font.family: c.font
            font.pixelSize: root.px(15)
            font.weight: Font.Medium
        }
    }

    // --- sign-in stage -------------------------------------------------------------
    Column {
        id: signinCol
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.height * 0.33 + (root.signin ? 0 : root.px(40))
        spacing: root.px(16)
        opacity: root.signin ? 1 : 0
        visible: opacity > 0
        enabled: root.signin
        transform: Translate { id: shakeX; x: 0 }
        Behavior on opacity { NumberAnimation { duration: 340; easing.type: Easing.OutCubic } }
        Behavior on y { NumberAnimation { duration: 420; easing.type: Easing.OutCubic } }

        SequentialAnimation {
            id: shake
            NumberAnimation { target: shakeX; property: "x"; to: -root.px(12); duration: 50 }
            NumberAnimation { target: shakeX; property: "x"; to: root.px(12); duration: 70 }
            NumberAnimation { target: shakeX; property: "x"; to: -root.px(7); duration: 60 }
            NumberAnimation { target: shakeX; property: "x"; to: root.px(4); duration: 50 }
            NumberAnimation { target: shakeX; property: "x"; to: 0; duration: 40 }
        }

        // Account picture (or initial) with a soft accent halo.
        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: root.px(120); height: width
            Rectangle {
                anchors.centerIn: parent
                width: parent.width + root.px(14); height: width; radius: width / 2
                color: "transparent"
                border.width: root.px(6)
                border.color: Qt.rgba(c.accentFg.r, c.accentFg.g, c.accentFg.b, 0.18)
            }
            Rectangle {
                id: avatar
                anchors.fill: parent
                radius: width / 2
                color: root.light ? Qt.rgba(0, 0, 0, 0.06) : Qt.rgba(1, 1, 1, 0.12)
                border.width: 2
                border.color: c.accentFg
                property string pic: root.userRole(userField.text, "ic")
                Image {
                    id: pic
                    anchors.fill: parent
                    anchors.margins: 3
                    source: avatar.pic.length > 0 ? "file://" + avatar.pic : ""
                    fillMode: Image.PreserveAspectCrop
                    visible: false
                }
                Rectangle {
                    id: picMask
                    anchors.fill: pic
                    radius: width / 2
                    visible: false
                    layer.enabled: true
                }
                MultiEffect {
                    anchors.fill: pic
                    source: pic
                    visible: pic.status === Image.Ready
                    maskEnabled: true
                    maskSource: picMask
                }
                Text {
                    anchors.centerIn: parent
                    visible: pic.status !== Image.Ready
                    text: (userField.text.length > 0 ? userField.text.charAt(0) : "?").toUpperCase()
                    color: c.textHi
                    font.family: c.font
                    font.pixelSize: root.px(48)
                    font.weight: Font.DemiBold
                }
            }
        }

        Text {   // display name
            anchors.horizontalCenter: parent.horizontalCenter
            visible: !root.askName
            text: root.displayName
            color: c.textHi
            font.family: c.font
            font.pixelSize: root.px(28)
            font.weight: Font.DemiBold
        }

        // Username entry: first boot, or "Other user".
        Rectangle {
            id: userBox
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.askName
            width: root.px(320); height: root.px(48); radius: height / 2
            color: c.glass
            border.width: userField.activeFocus ? 2 : 1
            border.color: userField.activeFocus ? c.accentFg : c.glassLine
            TextInput {
                id: userField
                objectName: "userField"
                anchors.fill: parent
                anchors.leftMargin: root.px(20); anchors.rightMargin: root.px(20)
                verticalAlignment: TextInput.AlignVCenter
                color: c.textHi
                selectionColor: c.accent
                selectedTextColor: c.onAccent
                font.family: c.font
                font.pixelSize: root.px(15)
                text: userModel.lastUser
                enabled: !root.busy
                KeyNavigation.tab: pwd
                Accessible.role: Accessible.EditableText
                Accessible.name: "Username"
                onAccepted: pwd.forceActiveFocus()
                onTextChanged: root.wake()
                Keys.onEscapePressed: root.toLock()
            }
            Text {
                anchors.left: parent.left; anchors.leftMargin: root.px(20)
                anchors.verticalCenter: parent.verticalCenter
                text: "Username"; color: c.hint
                font.family: c.font; font.pixelSize: root.px(15)
                visible: userField.text.length === 0
            }
        }

        // Glass password pill with reveal toggle and a round go button inside.
        Rectangle {
            id: pwdBox
            anchors.horizontalCenter: parent.horizontalCenter
            width: root.px(320); height: root.px(48); radius: height / 2
            color: c.glass
            border.width: pwd.activeFocus || root.errorText.length > 0 ? 2 : 1
            border.color: root.errorText.length > 0 ? c.danger
                        : pwd.activeFocus ? c.accentFg : c.glassLine
            Behavior on border.color { ColorAnimation { duration: 120 } }

            TextInput {
                id: pwd
                objectName: "password"
                property bool reveal: false
                anchors.left: parent.left; anchors.leftMargin: root.px(20)
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
                clip: true
                Accessible.role: Accessible.EditableText
                Accessible.name: "Password"
                Accessible.passwordEdit: !reveal
                onAccepted: root.doLogin()
                onTextChanged: { root.wake(); if (text.length > 0) root.errorText = "" }
                Keys.onEscapePressed: { if (text.length > 0) text = ""; else root.toLock() }
            }
            Text {
                anchors.left: parent.left; anchors.leftMargin: root.px(20)
                anchors.verticalCenter: parent.verticalCenter
                text: "Password"; color: c.hint
                font.family: c.font; font.pixelSize: root.px(15)
                visible: pwd.text.length === 0
            }

            Rectangle {   // show / hide password
                id: eye
                anchors.right: go.left; anchors.rightMargin: root.px(4)
                anchors.verticalCenter: parent.verticalCenter
                width: root.px(34); height: width; radius: width / 2
                color: eyeArea.containsMouse || activeFocus ? c.glassHi : "transparent"
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: pwd.reveal ? "Hide password" : "Show password"
                Keys.onSpacePressed: pwd.reveal = !pwd.reveal
                Keys.onReturnPressed: pwd.reveal = !pwd.reveal
                Item {
                    anchors.centerIn: parent; width: root.px(19); height: width
                    Glyph {   // password shown: an open eye
                        visible: pwd.reveal
                        tint: c.accentFg
                        PathSvg { path: "M2 12 Q12 3 22 12 Q12 21 2 12 M14.6 12 A2.6 2.6 0 1 1 9.4 12 A2.6 2.6 0 1 1 14.6 12" }
                    }
                    Glyph {   // password hidden: the eye struck through, with no pupil,
                              // so it never reads as a "no entry" sign
                        visible: !pwd.reveal
                        tint: c.textLo
                        PathSvg { path: "M2 12 Q12 3 22 12 Q12 21 2 12 M4 4 L20 20" }
                    }
                }
                MouseArea {
                    id: eyeArea; anchors.fill: parent; hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { pwd.reveal = !pwd.reveal; pwd.forceActiveFocus() }
                }
                FocusRing { target: eye }
            }

            Rectangle {   // go / busy
                id: go
                anchors.right: parent.right; anchors.rightMargin: root.px(6)
                anchors.verticalCenter: parent.verticalCenter
                width: root.px(36); height: width; radius: width / 2
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

        // One status line: error, busy, or caps-lock warning.
        Text {
            id: status
            anchors.horizontalCenter: parent.horizontalCenter
            width: root.px(360)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            font.family: c.font
            font.pixelSize: root.px(14)
            font.weight: Font.Medium
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

        // Other accounts (when there is more than one), plus "Other user".
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: root.px(8)
            Repeater {
                model: userModel.count > 1 ? userModel : 0
                delegate: Pill {
                    visible: index < 12
                    label: "Switch user"
                    value: model.realName.length > 0 ? model.realName : model.name
                    selected: userField.text === model.name && !root.askName
                    onActivated: {
                        root.askName = false
                        userField.text = model.name
                        pwd.text = ""
                        root.errorText = ""
                        pwd.forceActiveFocus()
                    }
                }
            }
            Pill {
                label: "Sign in as"
                value: root.askName ? "Back" : "Other user…"
                visible: userModel.lastUser.length > 0
                onActivated: {
                    root.askName = !root.askName
                    if (!root.askName) { userField.text = userModel.lastUser; pwd.forceActiveFocus() }
                    else { userField.text = ""; userField.forceActiveFocus() }
                }
            }
        }
    }

    // --- footer (sign-in stage): session + host left, power right ---------------
    Row {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: root.px(28)
        spacing: root.px(10)
        opacity: root.signin ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 300 } }

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
        anchors.margins: root.px(28)
        spacing: root.px(14)
        opacity: root.signin ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 300 } }

        IconButton {
            visible: sddm.canSuspend
            label: "Sleep"
            onActivated: sddm.suspend()
            icon: Glyph {
                tint: c.textHi
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
                tint: c.textHi
                // A 300 degree circle with a chevron that sits on the arc's tangent.
                PathSvg { path: "M16.59 5.45 A8 8 0 1 1 8.62 4.75 M3.75 3.62 L8.62 4.75 L6.35 9.20" }
            }
        }
        IconButton {
            visible: sddm.canPowerOff
            label: "Shut down"
            onActivated: sddm.powerOff()
            icon: Glyph {
                tint: c.textHi
                PathMove { x: 7; y: 6.5 }
                PathArc { x: 17; y: 6.5; radiusX: 8; radiusY: 8; useLargeArc: true; direction: PathArc.Counterclockwise }
                PathMove { x: 12; y: 2.5 }
                PathLine { x: 12; y: 11 }
            }
        }
    }

    // --- idle dim -----------------------------------------------------------------
    Rectangle {
        anchors.fill: parent
        color: "black"
        opacity: root.dimmed ? 0.75 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 700 } }
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
        onPositionChanged: root.wake()
    }
}
