// The format wheel: a ring of formats (or edits) around the cursor. See main.cpp.
import QtQuick
import QtQuick.Window
import QtQuick.Shapes

Window {
    id: win
    color: "transparent"
    visible: false

    readonly property real outerR: 190
    readonly property real innerR: 70
    property real cx: width / 2
    property real cy: height / 2
    property bool placed: false
    property bool clickMode: false
    property bool closing: false
    property bool edit: false
    property var files: []
    property var formats: []
    property var rest: []          // formats behind the "More" slot, when there are over 6
    readonly property int maxSlots: 6
    property int hover: -1
    property bool dark: true

    // A lightly translucent surface with a soft highlight along the top; KWin
    // blurs what is behind the ring. Nothing here needs a GPU.
    readonly property color surface: dark ? Qt.rgba(29 / 255, 32 / 255, 38 / 255, 0.80) : Qt.rgba(250 / 255, 251 / 255, 252 / 255, 0.82)
    readonly property color slotFill: dark ? Qt.rgba(1, 1, 1, 0.07) : Qt.rgba(0.07, 0.10, 0.18, 0.07)
    readonly property color hubFill: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(1, 1, 1, 0.62)
    readonly property color rim: dark ? Qt.rgba(1, 1, 1, 0.16) : Qt.rgba(1, 1, 1, 0.75)
    readonly property real gloss: dark ? 0.10 : 0.55
    readonly property color ink: dark ? "#ECEFF3" : "#14181F"
    readonly property color muted: dark ? "#A3ABB6" : "#5B6472"
    readonly property color accent: "#16C784"     // ArkaOS's own colour, not the desktop accent
    readonly property color accentInk: "#06281B"

    readonly property var names: ({ png: "PNG", jpg: "JPEG", webp: "WEBP", pdf: "PDF", mp3: "MP3", wav: "WAV",
                                    flac: "FLAC", ogg: "OGG", opus: "OPUS", m4a: "M4A", mp4: "MP4", mkv: "MKV",
                                    webm: "WEBM", "smaller-pdf": "SMALLER PDF", more: "MORE",
                                    crop: "Crop", redact: "Black out", background: "Background", "rotate-left": "Rotate left",
                                    "rotate-right": "Rotate right", split: "Split pages", merge: "Merge PDFs", trim: "Trim" })

    function label(f) {
        if (!edit && isPdf() && f === "png") return "PNG PAGES"
        if (!edit && isVideo() && f === "mp3") return "MP3 AUDIO"
        return names[f] || f.toUpperCase()
    }
    function ext() { return files.length ? String(files[0]).split(".").pop().toLowerCase() : "" }
    function isPdf() { return ext() === "pdf" }
    function isVideo() { return ["mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"].indexOf(ext()) >= 0 }
    function fileLabel() {
        if (!files.length) return ""
        if (files.length > 1) return files.length + " files"
        return decodeURIComponent(String(files[0]).split("/").pop())
    }
    function hubText() {
        if (hover >= 0 && hover < formats.length) return label(formats[hover])
        return clickMode ? "Pick one" : "Drop on one"
    }

    function listFor(urls) { return edit ? wheel.toolsFor(urls) : wheel.formatsFor(urls) }

    // Over maxSlots entries: the first five in priority order (the order the
    // program lists them in), and a "More" slot that shows the rest.
    function setFormats(list) {
        if (list.length > maxSlots) {
            formats = list.slice(0, maxSlots - 1).concat(["more"])
            rest = list.slice(maxSlots - 1)
        } else {
            formats = list
            rest = []
        }
    }
    // Pick slot i for these files: do it, or open the next ring for "More".
    function pick(i, urls) {
        if (formats[i] === "more") {
            files = urls; clickMode = true; hover = -1
            setFormats(rest)
        } else {
            if (edit) wheel.runTool(formats[i], urls)
            else wheel.convert(formats[i], urls)
            close()
        }
    }

    function open(urls, editMode) {
        closing = false; fade.stop(); ring.opacity = 0; ring.scale = 0.6
        dark = wheel.dark()
        edit = editMode
        clickMode = urls.length > 0
        files = urls
        setFormats(clickMode ? listFor(urls) : [])
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
    function stepBy(by) {
        const n = formats.length
        if (n) hover = hover < 0 ? (by > 0 ? 0 : n - 1) : (hover + by + n) % n
    }
    function close() {
        if (closing || !visible) return
        closing = true; placeTimer.stop(); leaveTimer.stop()
        fade.restart()
    }

    // One slot of the ring as an SVG path, with its corners cut at 45 degrees
    // like the ArkaOS popups. i is the slot, n the number of slots.
    function slotPath(i, n) {
        const c0 = outerR, R = outerR - 4, r = innerR + 4
        function P(rad, deg) {
            const a = deg * Math.PI / 180
            return (c0 + rad * Math.cos(a)).toFixed(2) + " " + (c0 + rad * Math.sin(a)).toFixed(2)
        }
        if (n <= 1)
            return "M " + (c0 - R) + " " + c0 + " A " + R + " " + R + " 0 1 1 " + (c0 + R) + " " + c0
                 + " A " + R + " " + R + " 0 1 1 " + (c0 - R) + " " + c0 + " Z"
                 + " M " + (c0 - r) + " " + c0 + " A " + r + " " + r + " 0 1 0 " + (c0 + r) + " " + c0
                 + " A " + r + " " + r + " 0 1 0 " + (c0 - r) + " " + c0 + " Z"
        const step = 360 / n, mid = -90 + i * step, cut = 14
        const g = (3 / ((R + r) / 2)) * 180 / Math.PI          // half of a 6 px gap between slots
        const a0 = mid - step / 2 + g, a1 = mid + step / 2 - g
        const co = cut / R * 180 / Math.PI, ci = cut / r * 180 / Math.PI
        return "M " + P(R, a0 + co) + " A " + R + " " + R + " 0 0 1 " + P(R, a1 - co)
             + " L " + P(R - cut, a1) + " L " + P(r + cut, a1) + " L " + P(r, a1 - ci)
             + " A " + r + " " + r + " 0 0 0 " + P(r, a0 + ci)
             + " L " + P(r + cut, a0) + " L " + P(R - cut, a0) + " Z"
    }

    Connections {
        target: wheel
        function onOpened(urls, editMode) { win.open(urls, editMode) }
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
    Shortcut { sequence: "Left"; enabled: win.clickMode; onActivated: win.stepBy(-1) }
    Shortcut { sequence: "Right"; enabled: win.clickMode; onActivated: win.stepBy(1) }
    Shortcut { sequences: ["Return", "Enter"]; enabled: win.clickMode && win.hover >= 0; onActivated: win.pick(win.hover, win.files) }

    // Drag mode: the file being dragged decides the formats.
    DropArea {
        anchors.fill: parent
        keys: ["text/uri-list"]
        onEntered: (drag) => {
            leaveTimer.stop()
            win.place(drag.x, drag.y)
            if (!win.files.length) { win.files = drag.urls; win.setFormats(win.listFor(drag.urls)) }
            win.hover = win.indexAt(drag.x, drag.y)
            drag.accept(Qt.CopyAction)
        }
        onPositionChanged: (drag) => { win.hover = win.indexAt(drag.x, drag.y) }
        onExited: leaveTimer.restart()
        onDropped: (drop) => {
            const i = win.indexAt(drop.x, drop.y)
            if (i >= 0) {
                drop.accept(Qt.CopyAction)   // copy, so the original file is never moved
                win.pick(i, drop.urls)
            } else {
                drop.accepted = false        // dropped off the ring: nothing happens to the file
                win.close()
            }
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
            if (win.clickMode && i >= 0) win.pick(i, win.files)
            else win.close()
        }
    }

    Item {
        id: ring
        x: win.cx - win.outerR; y: win.cy - win.outerR
        width: 2 * win.outerR; height: 2 * win.outerR
        visible: win.placed
        opacity: 0
        transformOrigin: Item.Center

        // A soft shadow: three faint discs, a little lower than the ring.
        Repeater {
            model: [10, 22, 36]
            delegate: Rectangle {
                required property int modelData
                x: -modelData; y: 8 - modelData
                width: ring.width + 2 * modelData; height: width; radius: width / 2
                color: Qt.rgba(0, 0, 0, win.dark ? 0.10 : 0.05)
            }
        }

        // The donut, hollow in the middle so nothing is picked by accident.
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: win.surface; strokeColor: win.rim; strokeWidth: 1
                fillRule: ShapePath.OddEvenFill
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: win.outerR - 0.5; radiusY: win.outerR - 0.5; startAngle: 0; sweepAngle: 360 }
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: win.innerR; radiusY: win.innerR; startAngle: 0; sweepAngle: 360; moveToStart: true }
            }
            ShapePath {   // the highlight along the top
                strokeColor: "transparent"
                fillRule: ShapePath.OddEvenFill
                fillGradient: LinearGradient {
                    x1: 0; y1: 0; x2: 0; y2: win.outerR
                    GradientStop { position: 0; color: Qt.rgba(1, 1, 1, win.dark ? 0.06 : 0.30) }
                    GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0) }
                }
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: win.outerR - 0.5; radiusY: win.outerR - 0.5; startAngle: 0; sweepAngle: 360 }
                PathAngleArc { centerX: win.outerR; centerY: win.outerR; radiusX: win.innerR; radiusY: win.innerR; startAngle: 0; sweepAngle: 360; moveToStart: true }
            }
        }

        Repeater {
            model: win.formats
            delegate: Item {
                id: slotItem
                required property int index
                required property string modelData
                readonly property bool on: win.hover === index
                readonly property real mid: (-90 + index * 360 / win.formats.length) * Math.PI / 180
                readonly property real r: (win.outerR + win.innerR) / 2
                readonly property real ym: win.outerR + r * Math.sin(mid)
                readonly property string path: win.slotPath(index, win.formats.length)
                anchors.fill: parent
                // The other slots step back while one is hovered.
                opacity: win.hover < 0 || on ? 1 : 0.45
                Behavior on opacity { NumberAnimation { duration: 100 } }

                Shape {
                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer
                    ShapePath {
                        strokeColor: "transparent"
                        fillRule: ShapePath.OddEvenFill
                        fillColor: slotItem.on ? win.accent : win.slotFill
                        PathSvg { path: slotItem.path }
                    }
                    ShapePath {   // gloss: light from above, fading out by the middle of the slot
                        strokeColor: "transparent"
                        fillRule: ShapePath.OddEvenFill
                        fillGradient: LinearGradient {
                            x1: 0; y1: slotItem.ym - 56; x2: 0; y2: slotItem.ym + 48
                            GradientStop { position: 0; color: Qt.rgba(1, 1, 1, slotItem.on ? 0.42 : win.gloss) }
                            GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0) }
                        }
                        PathSvg { path: slotItem.path }
                    }
                }
                Text {
                    width: 100
                    x: win.outerR + slotItem.r * Math.cos(slotItem.mid) - width / 2
                    y: slotItem.ym - height / 2
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    text: win.label(slotItem.modelData)
                    color: slotItem.on ? win.accentInk : win.ink
                    font.pixelSize: win.edit ? 15 : 17
                    font.weight: Font.Bold
                    font.letterSpacing: win.edit ? 0.2 : 0.6
                    scale: slotItem.on ? 1.22 : 1
                    Behavior on scale { SpringAnimation { spring: 5; damping: 0.4; epsilon: 0.002 } }
                }
            }
        }

        // The middle: what is chosen now, and which file.
        Rectangle {
            width: 2 * win.innerR - 14; height: width; radius: width / 2
            x: win.outerR - width / 2; y: win.outerR - height / 2
            color: win.hubFill
            visible: win.formats.length > 0
            Column {
                anchors.centerIn: parent
                spacing: 4
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: pillText.implicitWidth + 24; height: 28; radius: 14
                    color: win.hover >= 0 ? win.accent : "transparent"
                    gradient: win.hover >= 0 ? hoverGradient : null
                    Gradient {
                        id: hoverGradient
                        GradientStop { position: 0; color: "#5BE8B2" }
                        GradientStop { position: 0.6; color: "#16C784" }
                        GradientStop { position: 1; color: "#10A56C" }
                    }
                    Text {
                        id: pillText
                        anchors.centerIn: parent
                        text: win.hubText()
                        color: win.hover >= 0 ? win.accentInk : win.muted
                        font.pixelSize: win.hover >= 0 ? 13 : 12
                        font.weight: Font.Bold
                        font.letterSpacing: 0.4
                    }
                }
                Text {
                    width: 2 * win.innerR - 30
                    horizontalAlignment: Text.AlignHCenter
                    elide: Text.ElideMiddle
                    text: win.fileLabel()
                    color: win.muted
                    font.pixelSize: 11
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
        color: win.surface; border.color: win.rim
        Text {
            id: hint
            anchors.centerIn: parent
            color: win.ink; font.pixelSize: 13
            text: win.files.length ? (win.edit ? "This file has no edits yet" : "This file can't be converted yet")
                                   : "Drag a file onto the wheel"
        }
    }
}
