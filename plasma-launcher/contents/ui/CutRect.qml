/*
    A rectangle with its corners cut at 45 degrees: the shape of ArkaOS's
    popups (plasma-style/make-panel-svg.py), used for the launcher's search
    field, highlights, tiles and privacy line.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import QtQuick.Shapes

Shape {
    id: shape

    property real cut: 6
    property color color: "transparent"
    property color borderColor: "transparent"
    property real borderWidth: 0

    readonly property real c: Math.min(cut, width / 2, height / 2)
    // Half the border width, so the border is drawn inside the item.
    readonly property real o: borderWidth / 2

    preferredRendererType: Shape.CurveRenderer

    ShapePath {
        fillColor: shape.color
        strokeColor: shape.borderWidth > 0 ? shape.borderColor : "transparent"
        strokeWidth: shape.borderWidth > 0 ? shape.borderWidth : -1
        joinStyle: ShapePath.MiterJoin

        startX: shape.c + shape.o
        startY: shape.o
        PathLine { x: shape.width - shape.c - shape.o; y: shape.o }
        PathLine { x: shape.width - shape.o; y: shape.c + shape.o }
        PathLine { x: shape.width - shape.o; y: shape.height - shape.c - shape.o }
        PathLine { x: shape.width - shape.c - shape.o; y: shape.height - shape.o }
        PathLine { x: shape.c + shape.o; y: shape.height - shape.o }
        PathLine { x: shape.o; y: shape.height - shape.c - shape.o }
        PathLine { x: shape.o; y: shape.c + shape.o }
        PathLine { x: shape.c + shape.o; y: shape.o }
    }
}
