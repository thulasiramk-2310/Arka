import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    width: 1920
    height: 1080
    color: "#07080e"

    property string accent:  "#16c784"
    property string textHi:  "#f5f7fa"
    property string textLo:  "#9aa4b2"
    property string textDim: "#5b6675"

    // --- Background: ArkaOS wallpaper under a dark scrim ---------------------
    Image {
        anchors.fill: parent
        source: config.background ? config.background : ""
        fillMode: Image.PreserveAspectCrop
        opacity: 0.6
    }
    Rectangle { anchors.fill: parent; color: "#07080e"; opacity: 0.45 }

    // --- Clock (top-right) ---------------------------------------------------
    Column {
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 44
        spacing: 2
        Text {
            id: clock
            anchors.right: parent.right
            color: root.textHi
            font.pixelSize: 38
            font.weight: Font.Light
            function upd() { text = Qt.formatTime(new Date(), "hh:mm") }
            Component.onCompleted: upd()
        }
        Text {
            anchors.right: parent.right
            color: root.textLo
            font.pixelSize: 14
            text: Qt.formatDate(new Date(), "dddd, MMMM d")
        }
    }
    Timer { interval: 1000; running: true; repeat: true; onTriggered: clock.upd() }

    // --- Centre login panel --------------------------------------------------
    Column {
        id: panel
        anchors.centerIn: parent
        spacing: 16
        opacity: 0
        Component.onCompleted: fadeIn.start()
        NumberAnimation {
            id: fadeIn
            target: panel; property: "opacity"
            from: 0; to: 1; duration: 650; easing.type: Easing.OutCubic
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "ARKAOS"
            color: root.accent
            font.pixelSize: 52
            font.weight: Font.Bold
            font.letterSpacing: 6
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "YOUR COMPUTER IS YOURS"
            color: root.textLo
            font.pixelSize: 13
            font.letterSpacing: 3
        }

        Item { width: 1; height: 20 }

        // Username: prefilled from lastUser when SDDM knows it; editable so
        // the very first login (no state.conf yet — lastUser is empty) works.
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 300; height: 46; radius: 8
            color: "#161c24"
            border.width: 1
            border.color: userField.activeFocus ? root.accent : "#1e2630"
            TextInput {
                id: userField
                anchors.fill: parent
                anchors.leftMargin: 14; anchors.rightMargin: 14
                verticalAlignment: TextInput.AlignVCenter
                color: root.textHi
                font.pixelSize: 15
                text: userModel.lastUser
                focus: userModel.lastUser.length === 0
                onAccepted: pwd.forceActiveFocus()
            }
            Text {
                anchors.left: parent.left; anchors.leftMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                text: "Username"
                color: root.textDim
                font.pixelSize: 15
                visible: userField.text.length === 0
            }
        }

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 300; height: 46; radius: 8
            color: "#161c24"
            border.width: 1
            border.color: pwd.activeFocus ? root.accent : "#1e2630"
            TextInput {
                id: pwd
                anchors.fill: parent
                anchors.leftMargin: 14; anchors.rightMargin: 14
                verticalAlignment: TextInput.AlignVCenter
                color: root.textHi
                font.pixelSize: 15
                echoMode: TextInput.Password
                focus: userModel.lastUser.length > 0
                onAccepted: sddm.login(userField.text, pwd.text, sessionModel.lastIndex)
            }
            Text {
                anchors.left: parent.left; anchors.leftMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                text: "Password"
                color: root.textDim
                font.pixelSize: 15
                visible: pwd.text.length === 0
            }
        }

        Rectangle {
            id: loginBtn
            anchors.horizontalCenter: parent.horizontalCenter
            width: 300; height: 46; radius: 8
            color: btnArea.containsMouse ? "#1ed694" : root.accent
            Behavior on color { ColorAnimation { duration: 120 } }
            Text {
                anchors.centerIn: parent
                text: "Unlock"
                color: "#07080e"
                font.pixelSize: 15
                font.weight: Font.Bold
            }
            MouseArea {
                id: btnArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: sddm.login(userField.text, pwd.text, sessionModel.lastIndex)
            }
        }

        Text {
            id: errorMsg
            anchors.horizontalCenter: parent.horizontalCenter
            text: ""
            color: "#ff4d4f"
            font.pixelSize: 13
        }
    }

    // --- Power actions (bottom-right) ----------------------------------------
    Row {
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.margins: 32
        spacing: 22
        Text {
            text: "Restart"
            color: rsArea.containsMouse ? root.textHi : root.textLo
            font.pixelSize: 13
            MouseArea { id: rsArea; anchors.fill: parent; hoverEnabled: true; onClicked: sddm.reboot() }
        }
        Text {
            text: "Shut Down"
            color: sdArea.containsMouse ? "#ff4d4f" : root.textLo
            font.pixelSize: 13
            MouseArea { id: sdArea; anchors.fill: parent; hoverEnabled: true; onClicked: sddm.powerOff() }
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
