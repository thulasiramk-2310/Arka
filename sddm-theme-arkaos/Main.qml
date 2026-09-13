import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    width: 1920
    height: 1080
    color: "#07080e"

    property string accent:  "#16c784"
    property string accentHi: "#1ed694"
    property string textHi:  "#f5f7fa"
    property string textLo:  "#9aa4b2"
    property string textDim: "#5b6675"
    property string card:    "#0f141c"
    property string field:   "#161c24"
    property string line:    "#1e2630"
    property string fontFam:  "Inter"

    // --- Background: ArkaOS wallpaper under a dark, bottom-weighted scrim ------
    Image {
        anchors.fill: parent
        source: config.background ? config.background : ""
        fillMode: Image.PreserveAspectCrop
        opacity: 0.7
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#cc07080e" }
            GradientStop { position: 0.5; color: "#9907080e" }
            GradientStop { position: 1.0; color: "#e607080e" }
        }
    }

    // --- Clock + date (top-right) --------------------------------------------
    Column {
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 44
        spacing: 2
        Text {
            id: clock
            anchors.right: parent.right
            color: root.textHi
            font.family: root.fontFam
            font.pixelSize: 46
            font.weight: Font.Light
            function upd() { text = Qt.formatTime(new Date(), "hh:mm") }
            Component.onCompleted: upd()
        }
        Text {
            anchors.right: parent.right
            color: root.textLo
            font.family: root.fontFam
            font.pixelSize: 15
            text: Qt.formatDate(new Date(), "dddd, MMMM d")
        }
    }
    Timer { interval: 1000; running: true; repeat: true; onTriggered: clock.upd() }

    // --- Centre login card ----------------------------------------------------
    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: 360
        height: col.implicitHeight + 56
        radius: 20
        color: root.card
        opacity: 0
        border.width: 1
        border.color: root.line

        // subtle lift
        Rectangle {
            anchors.fill: parent; anchors.topMargin: 6
            radius: 20; color: "#4004060a"; z: -1
        }

        Component.onCompleted: fadeIn.start()
        NumberAnimation {
            id: fadeIn
            target: panel; property: "opacity"
            from: 0; to: 1; duration: 650; easing.type: Easing.OutCubic
        }

        Column {
            id: col
            anchors.centerIn: parent
            width: parent.width - 56
            spacing: 16

            // Circular avatar with the user's initial
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 76; height: 76; radius: 38
                color: root.field
                border.width: 2; border.color: root.accent
                Text {
                    anchors.centerIn: parent
                    text: (userField.text.length > 0 ? userField.text.charAt(0) : "a").toUpperCase()
                    color: root.accent
                    font.family: root.fontFam
                    font.pixelSize: 34
                    font.weight: Font.Bold
                }
            }

            // Restrained brand mark
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "ArkaOS"
                color: root.textHi
                font.family: root.fontFam
                font.pixelSize: 20
                font.weight: Font.DemiBold
                font.letterSpacing: 1
            }

            Item { width: 1; height: 4 }

            // Username: prefilled from lastUser; editable for the first login
            Rectangle {
                width: parent.width; height: 46; radius: 10
                color: root.field
                border.width: 1
                border.color: userField.activeFocus ? root.accent : root.line
                Behavior on border.color { ColorAnimation { duration: 120 } }
                TextInput {
                    id: userField
                    anchors.fill: parent
                    anchors.leftMargin: 14; anchors.rightMargin: 14
                    verticalAlignment: TextInput.AlignVCenter
                    color: root.textHi
                    font.family: root.fontFam
                    font.pixelSize: 15
                    text: userModel.lastUser
                    focus: userModel.lastUser.length === 0
                    onAccepted: pwd.forceActiveFocus()
                }
                Text {
                    anchors.left: parent.left; anchors.leftMargin: 14
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Username"; color: root.textDim
                    font.family: root.fontFam; font.pixelSize: 15
                    visible: userField.text.length === 0
                }
            }

            // Password with Caps-Lock hint
            Rectangle {
                width: parent.width; height: 46; radius: 10
                color: root.field
                border.width: 1
                border.color: pwd.activeFocus ? root.accent : root.line
                Behavior on border.color { ColorAnimation { duration: 120 } }
                TextInput {
                    id: pwd
                    anchors.fill: parent
                    anchors.leftMargin: 14; anchors.rightMargin: 40
                    verticalAlignment: TextInput.AlignVCenter
                    color: root.textHi
                    font.family: root.fontFam
                    font.pixelSize: 15
                    echoMode: TextInput.Password
                    focus: userModel.lastUser.length > 0
                    onAccepted: sddm.login(userField.text, pwd.text, sessionModel.lastIndex)
                }
                Text {
                    anchors.left: parent.left; anchors.leftMargin: 14
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Password"; color: root.textDim
                    font.family: root.fontFam; font.pixelSize: 15
                    visible: pwd.text.length === 0
                }
                // Caps-Lock indicator
                Text {
                    anchors.right: parent.right; anchors.rightMargin: 14
                    anchors.verticalCenter: parent.verticalCenter
                    text: "⇪"
                    color: root.accent
                    font.pixelSize: 16
                    visible: keyboard.capsLock
                }
            }

            // Unlock button
            Rectangle {
                id: loginBtn
                width: parent.width; height: 46; radius: 10
                color: btnArea.containsMouse ? root.accentHi : root.accent
                Behavior on color { ColorAnimation { duration: 120 } }
                Text {
                    anchors.centerIn: parent
                    text: "Unlock"
                    color: "#07080e"
                    font.family: root.fontFam
                    font.pixelSize: 15
                    font.weight: Font.Bold
                }
                MouseArea {
                    id: btnArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: sddm.login(userField.text, pwd.text, sessionModel.lastIndex)
                }
            }

            Text {
                id: errorMsg
                anchors.horizontalCenter: parent.horizontalCenter
                text: ""
                color: "#ff4d4f"
                font.family: root.fontFam
                font.pixelSize: 13
            }
        }
    }

    // --- Power actions (bottom-right, icon buttons) --------------------------
    Row {
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.margins: 32
        spacing: 14

        // Restart
        Rectangle {
            width: 42; height: 42; radius: 21
            color: rsArea.containsMouse ? root.field : "transparent"
            border.width: 1; border.color: root.line
            Behavior on color { ColorAnimation { duration: 120 } }
            Text {
                anchors.centerIn: parent; text: "⟳"
                color: rsArea.containsMouse ? root.textHi : root.textLo
                font.pixelSize: 20
            }
            MouseArea {
                id: rsArea; anchors.fill: parent; hoverEnabled: true
                cursorShape: Qt.PointingHandCursor; onClicked: sddm.reboot()
            }
        }
        // Shut down
        Rectangle {
            width: 42; height: 42; radius: 21
            color: sdArea.containsMouse ? "#33ff4d4f" : "transparent"
            border.width: 1; border.color: root.line
            Behavior on color { ColorAnimation { duration: 120 } }
            Text {
                anchors.centerIn: parent; text: "⏻"
                color: sdArea.containsMouse ? "#ff4d4f" : root.textLo
                font.pixelSize: 20
            }
            MouseArea {
                id: sdArea; anchors.fill: parent; hoverEnabled: true
                cursorShape: Qt.PointingHandCursor; onClicked: sddm.powerOff()
            }
        }
    }

    Connections {
        target: sddm
        function onLoginFailed() {
            errorMsg.text = "Incorrect password — try again"
            pwd.text = ""
            pwd.forceActiveFocus()
        }
    }
}
