// The format wheel: a ring of formats (or edits) around the cursor. See main.cpp.
import QtQuick
import QtQuick.Window
import QtQuick.Shapes

Window {
    id: win
    color: "transparent"
    visible: false

    // Sized from the screen, not in fixed pixels: about 30% of the screen height, never bigger than the
    // 380 px it was drawn at and never so small the labels stop fitting.
    readonly property real k: Math.max(0.72, Math.min(1.0, Screen.height * 0.30 / 380))
    readonly property real outerR: 190 * k
    readonly property real innerR: 70 * k
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

    // The ArkaOS look, as on the wallpaper: a flat field, hairlines, no gloss or glow.
    // Green appears once, as a short line on the slot you are on.
    readonly property color surface: dark ? Qt.rgba(20 / 255, 22 / 255, 27 / 255, 0.97) : Qt.rgba(244 / 255, 243 / 255, 238 / 255, 0.97)
    readonly property color slotFill: "transparent"
    readonly property color slotOnFill: dark ? Qt.rgba(1, 1, 1, 0.12) : Qt.rgba(0, 0, 0, 0.10)
    readonly property color idleInk: dark ? "#B4B8C2" : "#3F4755"
    readonly property color slotEdge: dark ? Qt.rgba(1, 1, 1, 0.16) : Qt.rgba(0, 0, 0, 0.18)
    readonly property color hubFill: dark ? Qt.rgba(20 / 255, 22 / 255, 27 / 255, 1) : Qt.rgba(244 / 255, 243 / 255, 238 / 255, 1)
    // A clear edge on any wallpaper: light on dark, dark on light.
    readonly property color rim: dark ? Qt.rgba(1, 1, 1, 0.34) : Qt.rgba(0, 0, 0, 0.46)
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
        return clickMode ? "Pick one" : (files.length ? "Drop on one" : "Drop a file")
    }

    // The dragged files: from the event, or parsed from the raw uri-list when the event leaves them out.
    function urlsFrom(ev) {
        if (ev.urls && ev.urls.length) return ev.urls
        const raw = ev.getDataAsString("text/uri-list")
        return raw ? raw.split(/\r?\n/).filter(l => l.length && l[0] !== "#") : []
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
        // The pointer may not move: fall back to the middle. With a file the ring is wanted at once;
        // without one it waits a little for a drag to start, then appears anyway and says what to do,
        // instead of leaving an invisible full-screen window.
        placeTimer.interval = clickMode ? 150 : 700
        placeTimer.restart()
    }
    function place(x, y) {
        if (placed) return
        cx = Math.max(outerR + 8, Math.min(width - outerR - 8, x))
        cy = Math.max(outerR + 8, Math.min(height - outerR - 8, y))
        placed = true
        wheel.setBlur(cx, cy, shapeR, innerR)
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
    // ── Cut-corner geometry ───────────────────────────────────────────────────
    // The wheel is an octagon, not a circle: flat sides with 45-degree cuts, the shape
    // the rest of ArkaOS uses. Everything below is in the ring item's own coordinates.
    readonly property real shapeR: outerR * 1.04      // the outline's corner radius

    function octPoints(rad) {          // eight corners, flat sides facing up/down/left/right
        const pts = []
        for (let q = 0; q < 8; q++) {
            const a = (22.5 + q * 45) * Math.PI / 180
            pts.push([rad * Math.cos(a), rad * Math.sin(a)])
        }
        return pts
    }
    function octEdge(rad, angle) {     // how far out the octagon's edge is in a direction
        const q = Math.PI / 4, phi = Math.round(angle / q) * q
        return rad * Math.cos(Math.PI / 8) / Math.cos(angle - phi)
    }
    function svg(pts, dy) {
        let d = ""
        for (let q = 0; q < pts.length; q++)
            d += (q ? " L " : "M ") + (outerR + pts[q][0]).toFixed(2) + " " + (outerR + pts[q][1] + (dy || 0)).toFixed(2)
        return d + " Z"
    }
    function octagonPath(rad, dy) { return svg(octPoints(rad), dy) }

    function clipHalf(pts, nx, ny, c) {   // keep the part where nx*x + ny*y >= c
        const out = []
        for (let q = 0; q < pts.length; q++) {
            const a = pts[q], b = pts[(q + 1) % pts.length]
            const da = nx * a[0] + ny * a[1] - c, db = nx * b[0] + ny * b[1] - c
            if (da >= 0) out.push(a)
            if ((da >= 0) !== (db >= 0)) {
                const t = da / (da - db)
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
            }
        }
        return out
    }
    function chain(pts) {   // the boundary part of a clipped octagon, without the sector's tip
        const far = innerR * 0.5
        const tip = pts.findIndex(p => Math.hypot(p[0], p[1]) < far)
        if (tip < 0) return pts
        return pts.slice(tip + 1).concat(pts.slice(0, tip))
    }
    function cutCorners(pts, corners, cut) {
        const out = []
        for (let q = 0; q < pts.length; q++) {
            if (corners.indexOf(q) < 0) { out.push(pts[q]); continue }
            const v = pts[q], pr = pts[(q + pts.length - 1) % pts.length], nx = pts[(q + 1) % pts.length]
            const lp = Math.hypot(pr[0] - v[0], pr[1] - v[1]), ln = Math.hypot(nx[0] - v[0], nx[1] - v[1])
            const cp = Math.min(cut, lp * 0.45), cn = Math.min(cut, ln * 0.45)
            out.push([v[0] + (pr[0] - v[0]) / lp * cp, v[1] + (pr[1] - v[1]) / lp * cp])
            out.push([v[0] + (nx[0] - v[0]) / ln * cn, v[1] + (nx[1] - v[1]) / ln * cn])
        }
        return out
    }
    // One slot: the part of the octagonal ring between two dividing lines. The sides stay
    // a constant 6 px apart and every corner has the same cut, so neighbours mirror each other.
    function slotPath(i, n) {
        const Ro = shapeR - 6.5, Ri = innerR + 6.5   // 6 px from the outline and from the middle, like the 6 px between slots
        if (n <= 1)
            return svg(octPoints(Ro)) + " " + svg(octPoints(Ri))
        const step = 360 / n, mid = -90 + i * step, d = 3
        const b0 = (mid - step / 2) * Math.PI / 180, b1 = (mid + step / 2) * Math.PI / 180
        const wedge = (pts) => clipHalf(clipHalf(pts, -Math.sin(b0), Math.cos(b0), d), Math.sin(b1), -Math.cos(b1), d)
        const outer = chain(wedge(octPoints(Ro))), inner = chain(wedge(octPoints(Ri))).reverse()
        const all = outer.concat(inner)
        const m = outer.length
        return svg(cutCorners(all, [0, m - 1, m, all.length - 1], 14 * k))
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
            const urls = win.urlsFrom(drag)
            if (!win.files.length && urls.length) { win.files = urls; win.setFormats(win.listFor(urls)) }
            win.hover = win.indexAt(drag.x, drag.y)
            drag.accept(Qt.CopyAction)
        }
        onPositionChanged: (drag) => { win.hover = win.indexAt(drag.x, drag.y) }
        onExited: leaveTimer.restart()
        onDropped: (drop) => {
            const i = win.indexAt(drop.x, drop.y)
            if (!win.formats.length) {
                // Some desktops only hand over the file when it is dropped. Take it, then let the user pick.
                const urls = win.urlsFrom(drop)
                drop.accept(Qt.CopyAction)
                if (urls.length) {
                    win.files = urls; win.clickMode = true; win.hover = -1
                    win.setFormats(win.listFor(urls))
                }
                if (!win.formats.length) win.close()
                return
            }
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

        // The donut, hollow in the middle so nothing is picked by accident.
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: win.surface; strokeColor: win.rim; strokeWidth: 1.5
                fillRule: ShapePath.OddEvenFill
                PathSvg { path: win.octagonPath(win.shapeR) }
                PathSvg { path: win.octagonPath(win.innerR) }
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

                Shape {
                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer
                    ShapePath {
                        strokeColor: slotItem.on ? (win.dark ? Qt.rgba(1, 1, 1, 0.45) : Qt.rgba(0, 0, 0, 0.45)) : win.slotEdge
                        strokeWidth: 1
                        joinStyle: ShapePath.MiterJoin
                        fillRule: ShapePath.OddEvenFill
                        fillColor: slotItem.on ? win.slotOnFill : win.slotFill
                        PathSvg { path: slotItem.path }
                    }
                }
                Text {
                    width: Math.max(72, 100 * win.k)
                    x: win.outerR + slotItem.r * Math.cos(slotItem.mid) - width / 2
                    y: slotItem.ym - height / 2
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    text: win.label(slotItem.modelData)
                    color: slotItem.on ? win.ink : win.idleInk
                    font.pixelSize: Math.max(12, (win.edit ? 13 : 14) * win.k)
                    font.weight: Font.DemiBold
                    font.letterSpacing: win.edit ? 0.6 : 1.4
                }
            }
        }

        // The middle: the file, and a green tick on its edge pointing at the slot you are on.
        Shape {
            anchors.fill: parent
            visible: win.formats.length > 0
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: win.hubFill; strokeColor: win.rim; strokeWidth: 1
                PathSvg { path: win.octagonPath(win.innerR) }
            }
        }
        Item {   // the pointer tick, turned to face the hovered slot
            visible: win.formats.length > 1 && win.hover >= 0
            readonly property real mid: (-90 + win.hover * 360 / Math.max(1, win.formats.length)) * Math.PI / 180
            readonly property real edgeR: win.octEdge(win.innerR, mid)   // the octagon's edge in that direction
            x: win.outerR + (edgeR + 6) * Math.cos(mid) - 6
            y: win.outerR + (edgeR + 6) * Math.sin(mid) - 1
            width: 12; height: 2
            rotation: mid * 180 / Math.PI
            transformOrigin: Item.Center
            Rectangle { anchors.fill: parent; color: win.accent }
        }
        Column {
            visible: win.formats.length > 0
            x: win.outerR - width / 2; y: win.outerR - height / 2
            width: 2 * win.innerR - 22
            spacing: 5
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: win.hover >= 0 ? win.ext().toUpperCase() : win.hubText()
                color: win.hover >= 0 ? win.muted : win.ink
                font.pixelSize: win.hover >= 0 ? 11 : 13
                font.weight: Font.DemiBold
                font.letterSpacing: 1.4
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideMiddle
                text: win.fileLabel()
                color: win.hover >= 0 ? win.ink : win.muted
                font.pixelSize: 12
            }
        }
    }

    // The ring is empty: say so in the middle of it, where the eye already is.
    Column {
        visible: win.placed && win.formats.length === 0
        opacity: ring.opacity
        x: win.cx - width / 2; y: win.cy - height / 2
        width: 2 * win.innerR - 22
        spacing: 5
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: win.files.length ? (win.edit ? "No edits" : "No formats") : "Drop a file"
            color: win.ink; font.pixelSize: 13; font.weight: Font.DemiBold; font.letterSpacing: 1.4
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideMiddle
            text: win.files.length ? win.fileLabel() : (win.edit ? "to edit it" : "to convert it")
            color: win.muted; font.pixelSize: 12
        }
    }

    // Shown only when there is nothing to pick.
    Rectangle {
        visible: win.placed && win.formats.length === 0
        opacity: ring.opacity
        x: win.cx - width / 2; y: win.cy + win.outerR + 12
        width: hint.implicitWidth + 28; height: 34; radius: 17
        antialiasing: true
        color: win.surface; border.color: win.rim
        Text {
            id: hint
            anchors.centerIn: parent
            color: win.ink; font.pixelSize: 13
            text: win.files.length ? (win.edit ? "Edits work on pictures, PDFs, sound and video"
                                                : "This file can't be converted yet")
                                   : "Drag a file onto the wheel, or press Esc"
        }
    }
}
