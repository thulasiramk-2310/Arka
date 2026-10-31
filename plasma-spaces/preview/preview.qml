// Standalone preview with fake data:
//   QT_QPA_PLATFORM=offscreen qml6 preview.qml   (writes /tmp/.../ridge.png, set OUT)
import QtQuick
import QtQuick.Window
import "../contents/ui"

Window {
    id: win
    visible: true
    width: 420
    height: 240
    color: "#17191d"

    property string out: Qt.application.arguments.length > 2 ? Qt.application.arguments[2] : "ridge.png"

    Column {
        anchors.centerIn: parent
        spacing: 14
        Repeater {
            model: [{n: 1, c: 0}, {n: 2, c: 1}, {n: 4, c: 0}, {n: 4, c: 2}]
            Rectangle {
                width: row.width + 24
                height: 34
                color: "#23262d"
                Row {
                    id: row
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 14
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Wed 28 Oct  1:17 AM"
                        color: "#ecEff3"
                        font.family: "Inter"; font.pixelSize: 13; font.weight: Font.Medium
                    }
                    Ridge {
                        count: modelData.n; current: modelData.c
                        anchors.verticalCenter: parent.verticalCenter
                        height: 22
                    }
                }
            }
        }
    }
    Timer {
        interval: 600; running: true
        onTriggered: win.contentItem.grabToImage(function (r) { r.saveToFile(win.out); Qt.quit() })
    }
}
