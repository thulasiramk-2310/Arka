// The format wheel: a ring of formats around the cursor. See main.cpp.
import QtQuick
import QtQuick.Window
import QtQuick.Shapes

Window {
    id: win
    color: "transparent"
    visible: false

    readonly property real outerR: 132
    readonly property real innerR: 46
    property real cx: width / 2
    property real cy: height / 2
    property bool placed: false
    property bool clickMode: false
    property bool closing: false
    property var files: []
    property var formats: []
    property int hover: -1
    property bool dark: true

    readonly property color bg: dark ? Qt.rgba(0x1E / 255, 0x1E / 255, 0x24 / 255, 0.70) : Qt.rgba(0xF4 / 255, 0xF4 / 255, 0xF6 / 255, 0.75)
    readonly property color edge: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.08)
    readonly property color ink: dark ? "#F2F2F5" : "#1D1D22"
    readonly property color accent: wheel.accent
    // Text on the accent: dark or light, whichever reads better.
    readonly property color onAccent: (0.299 * accent.r + 0.587 * accent.g + 0.114 * accent.b) > 0.6 ? "#111317" : "#FFFFFF"

    readonly property var names: ({ png: "PNG", jpg: "JPEG", webp: "WebP", pdf: "PDF", mp3: "MP3", wav: "WAV",
                                    flac: "FLAC", ogg: "OGG", m4a: "M4A", mp4: "MP4", mkv: "MKV", webm: "WebM",
                                    "smaller-pdf": "Smaller PDF" })
    readonly property var icons: ({ png: "image-png", jpg: "image-jpeg", webp: "image-webp", pdf: "application-pdf",
                                    mp3: "audio-mpeg", wav: "audio-x-wav", flac: "audio-x-flac", ogg: "audio-x-vorbis+ogg",
                                    m4a: "audio-mp4", mp4: "video-mp4", mkv: "video-x-matroska", webm: "video-webm",
                                    "smaller-pdf": "application-pdf" })

    function label(f) {
        if (isPdf() && f === "png") return "PNG pages"
        if (isVideo() && f === "mp3") return "MP3 audio"
        return names[f] || f.toUpperCase()
    }
    function ext() { return files.length ? String(files[0]).split(".").pop().toLowerCase() : "" }
    function isPdf() { return ext() === "pdf" }
    function isVideo() { return ["mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"].indexOf(ext()) >= 0 }

    function open(urls) {
        closing = false; fade.stop(); ring.opacity = 0; ring.scale = 0.6
        dark = wheel.dark()
        clickMode = urls.length > 0
        files = urls
        formats = clickMode ? wheel.formatsFor(urls) : []
        hover = -1; placed = false
        visible = true
        requestActivate()
        if (clickMode) placeTimer.restart()   // the pointer may not move: fall back to the middle
    }
    function place(x, y) {
        if (placed) return
        cx = Math.max(outerR + 8, Math.min(width - outerR - 8, x))
        cy = Math.max(outerR + 8, Math.min(height - outerR - 8, y))
        placed = true
        wheel.setBlur(cx, cy, outerR, innerR)
        enter.restart()
    }
    function indexAt(x, y) {
        const n = formats.length
        if (!n) return -1
        const dx = x - cx, dy = y - cy, d = Math.sqrt(dx * dx + dy * dy)
        if (d < innerR || d > outerR + 40) return -1
        const step = 360 / n
        let a = Math.atan2(dy, dx) * 180 / Math.PI + 90   // 0 = straight up
        return Math.round(((a % 360) + 360) % 360 / step) % n
    }
    function close() {
        if (closing || !visible) return
        closing = true; placeTimer.stop(); leaveTimer.stop()
        fade.restart()
    }

    Connections {
        target: wheel
        function onOpened(urls) { win.open(urls) }
    }
    Timer { id: placeTimer; interval: 150; onTriggered: win.place(win.width / 2, win.height / 2) }
    Timer { id: leaveTimer; interval: 300; onTriggered: win.close() }

    ParallelAnimation {
        id: enter
        NumberAnimation { target: ring; property: "scale"; from: 0.6; to: 1; duration: 150; easing.type: Easing.OutCubic }
        NumberAnimation { target: ring; property: "opacity"; from: 0; to: 1; duration: 150; easing.type: Easing.OutCubic }
    }
    SequentialAnimation {
        id: fade
        NumberAnimation { target: ring; property: "opacity"; to: 0; duration: 100 }
        ScriptAction { script: { wheel.clearBlur(); win.visible = false; win.closing = false } }
    }

    Shortcut { sequence: "Escape"; onActivated: win.close() }

    // Drag mode: the file being dragged decides the formats.
    DropArea {
        anchors.fill: parent
        keys: ["text/uri-list"]
        onEntered: (drag) => {
            leaveTimer.stop()
            win.place(drag.x, drag.y)
            if (!win.files.length) { win.files = drag.urls; win.formats = wheel.formatsFor(drag.urls) }
            win.hover = win.indexAt(drag.x, drag.y)
            drag.accept(Qt.CopyAction)
        }
        onPositionChanged: (drag) => { win.hover = win.indexAt(drag.x, drag.y) }
        onExited: leaveTimer.restart()
        onDropped: (drop) => {
            const i = win.indexAt(drop.x, drop.y)
            if (i >= 0) {
                wheel.convert(win.formats[i], drop.urls)
                drop.accept(Qt.CopyAction)   // copy, so the original file is never moved
            } else {
                drop.accepted = false
            }
            win.close()
        }
    }

    // Click mode, and placing the ring when the pointer first shows up.
    HoverHandler {
        id: pointer
        onPointChanged: {
            win.place(point.position.x, point.position.y)
            if (win.clickMode) win.hover = win.indexAt(point.position.x, point.position.y)
        }
    }
    TapHandler {
        onTapped: (eventPoint) => {
            const i = win.indexAt(eventPoint.position.x, eventPoint.position.y)
            if (win.clickMode && i >= 0) wheel.convert(win.formats[i], win.files)
            win.close()
        }
    }

    Item {
        id: ring
        x: win.cx - win.outerR; y: win.cy - win.outerR
        width: 2 * win.outerR; height: 2 * win.outerR
        visible: win.placed
        opacity: 0
        transformOrigin: Item.Center

        // The donut, hollow in the middle so nothing is picked by accident.
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: win.bg; strokeColor: win.edge; strokeWidth: 1
                fillRule: ShapePath.OddEvenFill
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: win.outerR - 0.5; radiusY: win.outerR - 0.5; startAngle: 0; sweepAngle: 360 }
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: win.innerR; radiusY: win.innerR; startAngle: 0; sweepAngle: 360; moveToStart: true }
            }
        }

        // The highlighted slot.
        Shape {
            id: slot
            anchors.fill: parent
            visible: win.hover >= 0 && win.formats.length > 0
            preferredRendererType: Shape.CurveRenderer
            readonly property real step: 360 / Math.max(1, win.formats.length)
            readonly property real gap: win.formats.length > 1 ? 1.5 : 0
            readonly property real a0: -90 + win.hover * step - step / 2 + gap
            readonly property real sweep: step - 2 * gap
            readonly property real r1: win.innerR + 5
            readonly property real r2: win.outerR - 5
            ShapePath {
                fillColor: win.accent; strokeColor: "transparent"
                startX: win.outerR + slot.r1 * Math.cos(slot.a0 * Math.PI / 180)
                startY: win.outerR + slot.r1 * Math.sin(slot.a0 * Math.PI / 180)
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: slot.r2; radiusY: slot.r2; startAngle: slot.a0; sweepAngle: slot.sweep; moveToStart: false }
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: slot.r1; radiusY: slot.r1; startAngle: slot.a0 + slot.sweep; sweepAngle: -slot.sweep; moveToStart: false }
            }
        }

        Repeater {
            model: win.formats
            delegate: Item {
                required property int index
                required property string modelData
                readonly property real mid: (-90 + index * 360 / win.formats.length) * Math.PI / 180
                readonly property real r: (win.outerR + win.innerR) / 2
                readonly property bool on: win.hover === index
                width: 72; height: 62
                x: win.outerR + r * Math.cos(mid) - width / 2
                y: win.outerR + r * Math.sin(mid) - height / 2
                scale: on ? 1.1 : 1
                Behavior on scale { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
                Image {
                    id: icon
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 4; width: 32; height: 32
                    sourceSize: Qt.size(64, 64)
                    source: "image://icon/" + (win.icons[modelData] || "text-x-generic")
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: icon.bottom; anchors.topMargin: 4
                    text: win.label(modelData)
                    color: parent.on ? win.onAccent : win.ink
                    font.pixelSize: 12; font.weight: Font.DemiBold
                }
            }
        }
    }

    // Shown only when there is nothing to pick.
    Rectangle {
        visible: win.placed && win.formats.length === 0
        opacity: ring.opacity
        x: win.cx - width / 2; y: win.cy + win.outerR + 12
        width: hint.implicitWidth + 28; height: 34; radius: 17
        color: win.bg; border.color: win.edge
        Text {
            id: hint
            anchors.centerIn: parent
            color: win.ink; font.pixelSize: 13
            text: win.files.length ? "This file can't be converted yet" : "Drag a file onto the wheel"
        }
    }
}
