/*
    The ridge: one hairline whose peaks are the virtual desktops. The current
    desktop's peak carries a ring with a dot in it. Pure drawing, no Plasma
    imports, so it can be previewed on its own (see ../../preview/).

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Shapes

Item {
    id: ridge

    property int count: 1
    property int current: 0            // zero-based index of the current desktop
    property color lineColor: "#8b919b"
    property color markColor: "#16c784"
    property string tooltipPrefix: ""  // unused by the drawing; kept for the host
    signal activated(int index)

    // Geometry in pixels: each desktop gets a slot; the peak is a 45-degree
    // triangle, like the peaks of the ArkaOS mark.
    readonly property int slot: 24
    readonly property int peak: 6
    // The drawing is 22 px tall and sits in the middle of whatever height it is given.
    readonly property int base: Math.round((height - 22) / 2) + 18
    readonly property int run: 6       // flat run either side of a peak, inside its slot

    implicitWidth: count * slot + 6
    implicitHeight: 22

    function ridgePoints() {
        var pts = [Qt.point(0, base)]
        for (var i = 0; i < count; i++) {
            var x0 = 3 + i * slot
            var cx = x0 + slot / 2
            pts.push(Qt.point(x0 + run, base))
            pts.push(Qt.point(cx, base - peak))
            pts.push(Qt.point(x0 + slot - run, base))
        }
        pts.push(Qt.point(width, base))
        return pts
    }

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: ridge.lineColor
            strokeWidth: 1
            fillColor: "transparent"
            joinStyle: ShapePath.MiterJoin
            PathPolyline { path: ridge.ridgePoints() }
        }
    }

    // The marker floats just above the current peak.
    Item {
        id: marker
        width: 8
        height: 8
        x: 3 + ridge.current * ridge.slot + ridge.slot / 2 - width / 2
        y: ridge.base - ridge.peak - height - 2
        visible: ridge.current >= 0 && ridge.current < ridge.count
        Behavior on x { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: "transparent"
            border.width: 1
            border.color: ridge.markColor
        }
        Rectangle {
            width: 3
            height: 3
            radius: 1.5
            anchors.centerIn: parent
            color: ridge.markColor
        }
    }

    Repeater {
        model: ridge.count
        MouseArea {
            x: 3 + index * ridge.slot
            width: ridge.slot
            height: ridge.height
            cursorShape: Qt.PointingHandCursor
            onClicked: ridge.activated(index)
        }
    }
}
