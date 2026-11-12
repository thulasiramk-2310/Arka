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
    readonly property real dockClear: 88
    property real cx: width / 2
    property real cy: height / 2
    property bool placed: false
    property bool clickMode: false
    property bool levelMode: false      // the second ring of "Make smaller": how much to give up
    property bool closing: false
    property bool edit: false
    property var files: []
    // The theme icons come from the program's image provider; a test harness without one shows words only.
    readonly property bool iconsReady: typeof wheel.hasIcons === "function" && wheel.hasIcons()
    readonly property string sizeLabel: files.length ? wheel.sizeText(files) : ""
    property var formats: []
    property var rest: []          // formats behind the "More" slot, when there are over 6
    readonly property int maxSlots: 6
    property int hover: -1
    property bool dark: true

    // The ArkaOS look, as on the wallpaper: a flat field, hairlines, no gloss or glow.
    // Green appears once, as a short line on the slot you are on.
    readonly property color surface: dark ? Qt.rgba(20 / 255, 22 / 255, 28 / 255, 0.97) : Qt.rgba(221 / 255, 225 / 255, 230 / 255, 0.97)
    readonly property color slotFill: dark ? "#1C1F26" : "#F7F8F9"
    // The slot you are on: a green tint as well as the green edge, so the choice does not rest on line weight.
    readonly property color slotOnFill: dark ? "#2A5A4C" : "#BFE0D1"
    readonly property color idleInk: dark ? "#D2D6DD" : "#2A313C"   // a slot not under the pointer: still clearly live
    readonly property color slotEdge: dark ? Qt.rgba(1, 1, 1, 0.32) : Qt.rgba(20 / 255, 24 / 255, 31 / 255, 0.46)
    readonly property color hubFill: dark ? Qt.rgba(20 / 255, 22 / 255, 27 / 255, 1) : Qt.rgba(241 / 255, 243 / 255, 245 / 255, 1)
    // A clear edge on any wallpaper: light on dark, dark on light.
    readonly property color rim: dark ? Qt.rgba(1, 1, 1, 0.45) : Qt.rgba(0, 0, 0, 0.52)
    readonly property color ink: dark ? "#ECEFF3" : "#14181F"
    readonly property color muted: dark ? "#A3ABB6" : "#5B6472"
    readonly property color accent: "#4DB491"     // ArkaOS's own colour, not the desktop accent
    // The ring around the slot you are on: on the light look the brand green is too pale for a mark that
    // must be seen (2.3:1), so a deeper green draws it (3.6:1).
    readonly property color accentEdge: dark ? accent : "#1F7A5C"
    readonly property color accentInk: "#06281B"

    // Sentence case everywhere; format names stay as they are written (PNG, MP3).
    readonly property var names: ({ png: "PNG", jpg: "JPEG", webp: "WebP", avif: "AVIF", tiff: "TIFF", bmp: "BMP", pdf: "PDF",
                                    mp3: "MP3", wav: "WAV", flac: "FLAC", ogg: "OGG", opus: "Opus", m4a: "M4A", mp4: "MP4",
                                    mkv: "MKV", webm: "WebM", "smaller-pdf": "Smaller PDF", "smaller-image": "Smaller",
                                    "smaller-audio": "Smaller", "smaller-video": "Smaller", zip: "ZIP", "zip-all": "Zip together",
                                    docx: "Word (rough)", text: "Text (OCR)", more: "More",
                                    "smaller:light": "Light", "smaller:medium": "Medium", "smaller:strong": "Strong",
                                    "smaller:10mb": "Under 10 MB", "smaller:25mb": "Under 25 MB",
                                    crop: "Crop", redact: "Black out", background: "Background", "rotate-left": "Rotate left",
                                    "rotate-right": "Rotate right", split: "Split pages", merge: "Merge PDFs", trim: "Trim",
                                    annotate: "Annotate", bleep: "Bleep", "split-media": "Split", join: "Join in name order",
                                    smaller: "Smaller", clean: "Remove details", "hide-private": "Find private text",
                                    odt: "ODT", "open-editor": "Text editor", extract: "Extract here", "pdf-combine": "One PDF",
                                    searchable: "Searchable", resize: "Resize", "copy-path": "Copy path",
                                    "resize:50%": "Half size", "resize:1920": "1920 wide", "resize:1280": "1280 wide",
                                    "resize:800": "800 wide" })
    // A theme icon for each edit, so a slot is recognised before it is read. Formats stay as words.
    readonly property var icons: ({ more: "overflow-menu", crop: "transform-crop", annotate: "draw-freehand", redact: "view-hidden",
                                    background: "color-fill", smaller: "view-restore", clean: "document-cleanup", "hide-private": "document-scan",
                                    "rotate-left": "object-rotate-left", "rotate-right": "object-rotate-right", "open-editor": "document-edit",
                                    extract: "archive-extract", "pdf-combine": "x-office-document", searchable: "document-scan",
                                    resize: "transform-scale", "copy-path": "edit-copy",
                                    trim: "edit-cut", "split-media": "view-split-left-right", split: "view-split-left-right",
                                    bleep: "audio-volume-muted", join: "media-playlist-append", merge: "media-playlist-append",
                                    "zip-all": "archive-insert", zip: "archive-insert" })

    function label(f) {
        if (!edit && isPdf() && f === "png") return "PNG pages"
        if (!edit && isVideo() && f === "mp3") return "MP3 audio"
        if (!edit && isText() && f === "docx") return "Word"   // from plain text nothing is lost
        if (!edit && f === "zip" && files.length > 1) return "ZIP each"
        return names[f] || f.toUpperCase()   // an unknown format name is written as formats are
    }
    // The file-type icon of a format slot, from the icon theme (the generic one where the theme has none).
    function formatIcon(f) {
        const m = { png: "image-png", jpg: "image-jpeg", tiff: "image-tiff", bmp: "image-bmp", webp: "image-x-generic",
                    avif: "image-x-generic", pdf: "x-office-document", "smaller-pdf": "x-office-document", mp3: "audio-mpeg",
                    wav: "audio-x-wav", flac: "audio-flac", ogg: "audio-ogg", opus: "audio-x-generic", m4a: "audio-mp4",
                    mp4: "video-mp4", mkv: "video-x-matroska", webm: "video-webm", zip: "application-zip",
                    "zip-all": "application-zip", docx: "x-office-document", odt: "x-office-document", text: "text-x-generic" }
        if (m[f]) return m[f]
        if (String(f).indexOf("smaller") === 0) return "view-restore"
        return ""
    }
    // What a slot does, for screen readers (and anyone who wonders before picking).
    function describe(f) {
        const d = { crop: "Open the crop window", annotate: "Open the drawing window", redact: "Open the black-out window",
                    "hide-private": "Open the black-out window and suggest boxes over emails, phone and card numbers",
                    background: "Open the background window", smaller: "Choose how much smaller to make it",
                    clean: "Save a copy without hidden details such as location", trim: "Open the trim window",
                    "split-media": "Open the split window", bleep: "Open the bleep window", join: "Join them in name order",
                    merge: "Merge the PDFs in name order", more: "Show more choices", split: "Save each page as its own PDF",
                    "rotate-left": "Save a copy turned left", "rotate-right": "Save a copy turned right",
                    "open-editor": "Open it in the text editor", extract: "Unpack it into a folder beside it",
                    "pdf-combine": "Put the pictures into one PDF, in the order you picked them",
                    searchable: "Save a copy whose words can be found and copied", resize: "Choose a width",
                    "copy-path": "Copy where the file is to the clipboard" }
        if (d[f]) return d[f]
        if (String(f).indexOf("smaller") === 0) return "Save a smaller copy"
        return "Save a copy as " + label(f)
    }
    function ext() { return files.length ? String(files[0]).split(".").pop().toLowerCase() : "" }
    function isPdf() { return ext() === "pdf" }
    function isText() { return ["txt", "text", "log", "md", "markdown", "rst", "csv", "tsv", "json", "xml", "yaml", "yml",
                                "ini", "conf", "cfg"].indexOf(ext()) >= 0 }
    // iPhone photos: the HEVC decoder is not in Fedora (patented), so there is no honest way to offer them yet.
    function isHeic() { return ["heic", "heif"].indexOf(ext()) >= 0 }
    // What the hint says when the ring is empty.
    function emptyHint() {
        if (!files.length) return "Move the file you are dragging over the wheel, or press Esc"
        if (edit) return "Edits work on pictures, PDFs, sound, video and text files"
        if (isHeic()) return "iPhone HEIC photos can't be converted yet: Fedora has no decoder for them"
        return "This file can't be converted yet"
    }
    function isVideo() { return ["mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"].indexOf(ext()) >= 0 }
    function fileLabel() {
        if (!files.length) return ""
        if (files.length > 1) return files.length + " files"
        const name = String(files[0]).split("/").pop()
        try { return decodeURIComponent(name) } catch (e) { return name }
    }
    readonly property var effects: ({ crop: "Choose what to keep", annotate: "Draw and write on it", redact: "Cover parts in black",
                                      "hide-private": "Suggest boxes over private text", background: "Set it on a colour",
                                      smaller: "Choose how much smaller", clean: "A copy without hidden details",
                                      "rotate-left": "A copy turned left", "rotate-right": "A copy turned right", "open-editor": "Change the words", extract: "Unpack into a folder",
                                      "pdf-combine": "A page per picture", searchable: "Words you can find",
                                      resize: "Choose a width", "copy-path": "Paste it anywhere",
                                      trim: "Keep one part", "split-media": "Cut it in two", bleep: "A tone over words",
                                      join: "One file, in name order", merge: "One PDF, in name order",
                                      split: "One PDF per page", more: "More choices", zip: "Put it in a ZIP",
                                      "zip-all": "All in one ZIP", "smaller-image": "A smaller copy", "smaller-pdf": "A smaller copy",
                                      "smaller-audio": "A smaller copy", "smaller-video": "A smaller copy" })
    readonly property var targets: ["png", "jpg", "webp", "avif", "tiff", "bmp", "pdf", "mp3", "wav", "flac", "ogg", "opus",
                                    "m4a", "mp4", "mkv", "webm", "docx", "text"]
    function hubText() {
        // The middle says what picking the slot does, in other words than the slot's own label.
        if (hover >= 0 && hover < formats.length) {
            const f = formats[hover]
            if (!edit && targets.indexOf(f) >= 0) return "Make a " + label(f) + " copy"
            return effects[f] || effects[String(f).split(":")[0]] || label(f)
        }
        return clickMode ? "Pick one" : (files.length ? "Drop on one" : "Drop a file")   // nothing hovered yet
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
        // Ignore a second click/Enter while the first choice is fading out.
        if (closing || !visible || i < 0 || i >= formats.length || !urls.length) return
        if (formats[i] === "more") {
            files = urls; clickMode = true; hover = -1
            setFormats(rest)
        } else if ((!edit && String(formats[i]).indexOf("smaller-") === 0) || (edit && formats[i] === "smaller")) {
            // "Make smaller" asks how much first: a second ring, like More. Light is what it has always done.
            files = urls; clickMode = true; hover = -1; levelMode = true
            // A size to reach, offered only when the smallest file is bigger than it.
            const big = urls.every(u => wheel.totalBytes([u]) > 10000000), huge = urls.every(u => wheel.totalBytes([u]) > 25000000)
            formats = ["smaller:light", "smaller:medium", "smaller:strong"]
                      .concat(big ? ["smaller:10mb"] : []).concat(huge ? ["smaller:25mb"] : []); rest = []
        } else if (edit && formats[i] === "resize") {
            // Resize asks for a width first, like Smaller asks how much.
            files = urls; clickMode = true; hover = -1; levelMode = true
            formats = ["resize:50%", "resize:1920", "resize:1280", "resize:800"]; rest = []
        } else {
            // The level ring's choices convert, also when it was opened from the edit wheel.
            if (edit && String(formats[i]).indexOf("smaller:") !== 0) wheel.runTool(formats[i], urls)
            else wheel.convert(formats[i], urls)
            close()
        }
    }

    function open(urls, editMode) {
        closing = false; fade.stop(); enter.stop(); leaveTimer.stop(); ring.opacity = 0; ring.scale = 0.6
        dark = wheel.dark()
        edit = editMode
        levelMode = false
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
        // Keep clear of the dock at the bottom: it reserves no space (it hides under windows), so near the
        // bottom edge the ring covered it (booted image, 2026-11-06). About 80 px tall at 1080p.
        cy = Math.max(outerR + 8, Math.min(height - outerR - dockClear, y))
        placed = true
        wheel.setBlur(cx, cy, shapeR, innerR)
        enter.restart()
    }
    function indexAt(x, y) {
        const n = formats.length
        if (!n) return -1
        const dx = x - cx, dy = y - cy, d = Math.sqrt(dx * dx + dy * dy)
        const angle = Math.atan2(dy, dx)
        // Keep the central dead zone, and reject the transparent area outside
        // the visible octagon instead of silently choosing its nearest slot.
        if (d < innerR || d > octEdge(shapeR, angle)) return -1
        const step = 360 / n
        let a = angle * 180 / Math.PI + 90   // 0 = straight up
        return Math.round(((a % 360) + 360) % 360 / step) % n
    }
    function stepBy(by) {
        const n = formats.length
        if (n) hover = hover < 0 ? (by > 0 ? 0 : n - 1) : (hover + by + n) % n
    }
    // Home and End: the first and the last slot.
    function jumpTo(last) {
        const n = formats.length
        if (n) hover = last ? n - 1 : 0
    }
    function close() {
        if (closing || !visible) return
        closing = true; placeTimer.stop(); leaveTimer.stop()
        fade.restart()
    }
    function handleDragExit() {
        // A drop on More (or a late URI list) has become click mode. Do not
        // dismiss its choices when that finished drag leaves the window.
        if (!clickMode && !closing) leaveTimer.restart()
    }

    function handleDrop(drop) {
        if (closing) { drop.accepted = false; return }
        const urls = urlsFrom(drop)
        if (!urls.length) { drop.accepted = false; close(); return }
        const i = indexAt(drop.x, drop.y)
        const available = listFor(urls)
        if (!formats.length || (i >= 0 && formats[i] !== "more" && available.indexOf(formats[i]) < 0)) {
            // Late URI delivery or changed files: display the actual drop's choices.
            drop.accept(Qt.CopyAction)
            files = urls; clickMode = true; hover = -1
            setFormats(available)
            if (!formats.length) close()
            return
        }
        if (i >= 0) {
            drop.accept(Qt.CopyAction)
            pick(i, urls)
        } else {
            drop.accepted = false
            close()
        }
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
    function trimShort(pts, t) {
        const d = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1])
        while (pts.length > 2 && d(pts[0], pts[1]) < t) pts.splice(1, 1)
        while (pts.length > 2 && d(pts[pts.length - 1], pts[pts.length - 2]) < t) pts.splice(pts.length - 2, 1)
        return pts
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
        // A cut that lands just past an octagon corner leaves a sliver of the next side: a kink a few pixels
        // long. Drop such slivers, so every slot edge is a clean line.
        const outer = trimShort(chain(wedge(octPoints(Ro))), 10 * k), inner = trimShort(chain(wedge(octPoints(Ri))), 10 * k).reverse()
        const all = outer.concat(inner)
        const m = outer.length
        // Only the outer corners are cut: on the short inner edge a cut left a small kink at each end.
        return svg(cutCorners(all, [0, m - 1], 14 * k))
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
    // Everything the pointer does can be done from the keyboard: the arrows (or Tab) move round the ring,
    // Home and End jump to its ends, Enter or Space picks the slot you are on.
    Shortcut { sequences: ["Left", "Up", "Backtab"]; enabled: win.clickMode; onActivated: win.stepBy(-1) }
    Shortcut { sequences: ["Right", "Down", "Tab"]; enabled: win.clickMode; onActivated: win.stepBy(1) }
    Shortcut { sequence: "Home"; enabled: win.clickMode; onActivated: win.jumpTo(false) }
    Shortcut { sequence: "End"; enabled: win.clickMode; onActivated: win.jumpTo(true) }
    Shortcut { sequences: ["Return", "Enter", "Space"]; enabled: win.clickMode && win.hover >= 0; onActivated: win.pick(win.hover, win.files) }

    // Drag mode: the file being dragged decides the formats.
    DropArea {
        anchors.fill: parent
        keys: ["text/uri-list"]
        onEntered: (drag) => {
            leaveTimer.stop()
            win.place(drag.x, drag.y)
            const urls = win.urlsFrom(drag)
            if (!win.clickMode && urls.length) { win.files = urls; win.setFormats(win.listFor(urls)) }
            win.hover = win.indexAt(drag.x, drag.y)
            drag.accept(Qt.CopyAction)
        }
        onPositionChanged: (drag) => { win.hover = win.indexAt(drag.x, drag.y) }
        onExited: win.handleDragExit()
        onDropped: (drop) => win.handleDrop(drop)
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

        // A soft shadow under the ring, so it sits above the windows behind it. Stacked translucent
        // octagons, because the wheel draws without a GPU, where blur effects are not available.
        Repeater {
            model: 16
            delegate: Shape {
                required property int index
                anchors.fill: parent
                ShapePath {
                    fillColor: Qt.rgba(20 / 255, 24 / 255, 31 / 255, win.dark ? 0.032 : 0.012); strokeColor: "transparent"
                    PathSvg { path: win.octagonPath(win.shapeR - 4 + index * 1.6, 5) }
                }
            }
        }

        // The donut, hollow in the middle so nothing is picked by accident.
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            // The hollow is only for the pointer (nothing there can be picked). It is filled solid, because the
            // text in the middle is drawn over whatever is behind the window, and a selected file showing
            // through made it unreadable.
            ShapePath {
                fillColor: Qt.rgba(win.surface.r, win.surface.g, win.surface.b, 1); strokeColor: "transparent"
                PathSvg { path: win.octagonPath(win.innerR) }
            }
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

                // What a screen reader says and does for this slot.
                Accessible.role: Accessible.Button
                Accessible.name: win.label(slotItem.modelData)
                Accessible.description: win.describe(slotItem.modelData)
                Accessible.focused: slotItem.on
                Accessible.onPressAction: win.pick(slotItem.index, win.files)

                Shape {
                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer
                    ShapePath {
                        // The slot you are on: an accent edge, clear for low vision too.
                        strokeColor: slotItem.on ? win.accentEdge : win.slotEdge
                        strokeWidth: slotItem.on ? 2 : 1
                        joinStyle: ShapePath.RoundJoin     // a miter left spurs at the sharp inner corners
                        fillRule: ShapePath.OddEvenFill
                        fillColor: slotItem.on ? win.slotOnFill : win.slotFill
                        PathSvg { path: slotItem.path }
                    }
                }
                Column {
                    // Actions get an icon; a format is its name, set large (the theme's file icons looked alike).
                    readonly property bool isFormat: win.targets.indexOf(slotItem.modelData) >= 0
                    readonly property string iconName: win.edit ? (win.icons[slotItem.modelData] || "") : ""
                    // The edit wheel's is wide enough for "Find private text" on one line in a top or bottom
                    // slot. Slots to the sides are narrower, and the format wheel's words are larger, so there a
                    // long label ("Zip together", "Remove details") wraps instead of touching the slot's edges.
                    width: (win.edit ? Math.max(118, 128 * win.k) : Math.max(92, 100 * win.k))
                           * (Math.abs(Math.sin(slotItem.mid)) > 0.9 ? 1 : 0.75)
                    x: win.outerR + slotItem.r * Math.cos(slotItem.mid) - width / 2
                    y: slotItem.ym - height / 2
                    spacing: 5
                    Item {   // "More": three dots in the label's colour, clearer than the theme's thin glyph
                        visible: slotItem.modelData === "more" && win.edit   // the format wheel is words only
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.max(24, Math.round(28 * win.k)); height: width
                        Row {
                            anchors.centerIn: parent; spacing: 4
                            Repeater { model: 3; Rectangle { width: 5; height: 5; radius: 2.5
                                                             color: slotItem.on ? win.ink : win.idleInk } }
                        }
                    }
                    Image {
                        visible: parent.iconName.length > 0 && slotItem.modelData !== "more"
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.max(24, Math.round(28 * win.k)); height: width
                        sourceSize: Qt.size(width * 2, height * 2)
                        // Left empty (the space kept, so every slot lines up) when the theme lacks the icon.
                        source: parent.iconName.length && win.iconsReady
                                && (typeof wheel.hasIcon !== "function" || wheel.hasIcon(parent.iconName))
                                ? "image://icon/" + parent.iconName + "?tint=" + String(slotItem.on ? win.ink : win.idleInk).substr(1) : ""
                    }
                    Text { textFormat: Text.PlainText;
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        text: win.label(slotItem.modelData)
                        color: slotItem.on ? win.ink : win.idleInk
                        // One size per ring: the format wheel is words, so they are set larger; the edit wheel has icons.
                        font.pixelSize: win.edit ? Math.max(13, 14 * win.k) : Math.max(15, 16 * win.k)
                        font.weight: Font.DemiBold   // the same when chosen: the fill and edge say which
                        font.letterSpacing: text.length > 12 ? 0 : 0.3   // long labels keep clear of the slot's edges
                    }
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
            visible: false   // the hovered slot is already marked by its fill; a separate tick read as a glitch
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
            Text { textFormat: Text.PlainText;
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                text: win.hubText()
                color: win.ink
                font.pixelSize: 13
                font.weight: Font.DemiBold
                font.letterSpacing: 0.3
            }
            Text { textFormat: Text.PlainText;
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideMiddle
                text: win.fileLabel()
                color: win.muted
                font.pixelSize: 12
            }
            Text { textFormat: Text.PlainText;   // how big it is now, so "Smaller" has something to be compared with
                width: parent.width
                visible: text.length > 0
                horizontalAlignment: Text.AlignHCenter
                text: win.sizeLabel
                color: win.muted
                font.pixelSize: 12
                font.letterSpacing: 0.4
            }
        }
    }

    // Tells keyboard users what works, so the keys are not a secret. Under the ring, or above it when
    // the ring sits at the bottom edge of the screen.
    Rectangle {
        id: keyHint
        visible: win.placed && win.clickMode && win.formats.length > 0
        opacity: ring.opacity
        x: Math.max(8, Math.min(win.width - width - 8, win.cx - width / 2))
        y: win.cy + win.outerR + 12 + height > win.height - 6 ? win.cy - win.outerR - 12 - height
                                                              : win.cy + win.outerR + 12
        width: keyHintText.implicitWidth + 28; height: 32
        color: "transparent"
        // The ring's own shape language: corners cut at 45 degrees, not a round pill.
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: win.surface; strokeColor: win.rim; strokeWidth: 1; joinStyle: ShapePath.MiterJoin
                readonly property real c: 9
                readonly property real w: keyHint.width - 0.5
                readonly property real h: keyHint.height - 0.5
                startX: c; startY: 0.5
                PathLine { x: keyHint.width - 9.5; y: 0.5 }
                PathLine { x: keyHint.width - 0.5; y: 9.5 }
                PathLine { x: keyHint.width - 0.5; y: keyHint.height - 9.5 }
                PathLine { x: keyHint.width - 9.5; y: keyHint.height - 0.5 }
                PathLine { x: 9.5; y: keyHint.height - 0.5 }
                PathLine { x: 0.5; y: keyHint.height - 9.5 }
                PathLine { x: 0.5; y: 9.5 }
                PathLine { x: 9.5; y: 0.5 }
            }
        }
        Text { textFormat: Text.PlainText;
            id: keyHintText
            anchors.centerIn: parent
            color: win.ink; font.pixelSize: 13
            text: win.levelMode ? "Light keeps the most quality, Strong makes the smallest file; a size is kept only if reached. Esc to cancel"
                                : "Click, or use the arrow keys and Enter · Esc to cancel"
        }
    }

    // The ring is empty: say so in the middle of it, where the eye already is.
    Column {
        visible: win.placed && win.formats.length === 0
        opacity: ring.opacity
        x: win.cx - width / 2; y: win.cy - height / 2
        width: 2 * win.innerR - 22
        spacing: 5
        Text { textFormat: Text.PlainText;
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: win.files.length ? (win.edit ? "No edits" : "No formats") : "Move the file"
            color: win.ink; font.pixelSize: 13; font.weight: Font.DemiBold; font.letterSpacing: 0.3
        }
        Text { textFormat: Text.PlainText;
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideMiddle
            text: win.files.length ? win.fileLabel() : "onto the wheel"
            color: win.muted; font.pixelSize: 12
        }
    }

    // Shown only when there is nothing to pick.
    Rectangle {
        visible: win.placed && win.formats.length === 0
        opacity: ring.opacity
        x: Math.max(8, Math.min(win.width - width - 8, win.cx - width / 2)); y: win.cy + win.outerR + 12
        width: hint.implicitWidth + 28; height: 34; radius: 17
        antialiasing: true
        color: win.surface; border.color: win.rim
        Text { textFormat: Text.PlainText;
            id: hint
            anchors.centerIn: parent
            color: win.ink; font.pixelSize: 13
            text: win.emptyHint()
        }
    }

    // The way that needs no dragging: the files come with the click, so the options are there at once.
    Text { textFormat: Text.PlainText;
        visible: win.placed && win.formats.length === 0 && win.files.length === 0
        opacity: ring.opacity
        x: Math.max(8, Math.min(win.width - width - 8, win.cx - width / 2)); y: win.cy + win.outerR + 12 + 34 + 10
        color: win.muted; font.pixelSize: 12
        text: win.edit ? "Or right-click a file (or press the Menu key) and choose Edit with the edit wheel"
                       : "Or right-click a file (or press the Menu key) and choose Convert with the format wheel"
    }
}
