// arka-edit: crop, black out, draw on, add a background, trim, split, beep. See main.cpp.
import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Dialogs

Window {
    id: win
    // Sized once: hiding the details later widens the picture, the window stays where it is.
    width: 780
    Component.onCompleted: {
        if (edit.inspectorOpen) width = 1060
        // "Find private text" on the wheel: start by suggesting boxes; the person still checks and saves.
        if (edit.autoFind && edit.tool === "redact") { findBtn.busy = true; edit.findPrivate() }
    }
    height: 720
    minimumWidth: 720
    minimumHeight: 520
    visible: true
    title: ({ crop: edit.isVideo ? "Crop video" : "Crop picture", redact: "Black out parts", annotate: "Draw on it",
              background: "Add a background", trim: "Trim", split: "Split in two", bleep: "Bleep out parts" })[edit.tool]
           + " — " + edit.fileName
    color: bg

    readonly property bool dark: edit.dark()
    readonly property color bg: dark ? "#1B1E23" : "#F1F3F5"
    readonly property color panel: dark ? "#23272E" : "#FFFFFF"
    readonly property color ink: dark ? "#ECEFF3" : "#14181F"
    readonly property color muted: dark ? "#A3ABB6" : "#5B6472"
    readonly property color line: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0.07, 0.1, 0.15, 0.12)
    // The edge of something to type in or pick: at least 3:1 against the page and the panel (WCAG 1.4.11).
    readonly property color edge: dark ? "#6B7380" : "#737B87"
    // The edge of a secondary button: stronger than a divider, so a dim button still reads as a button.
    readonly property color btnEdge: edge
    readonly property color accent: "#4DB491"
    readonly property color accentInk: "#06281B"
    // The green as text, and as a ring that marks focus or a choice: on the light look the brand green is
    // too pale for that (2.3:1), so a deeper green carries it (4.7:1 and 3.6:1 against the page).
    readonly property color accentText: dark ? accent : "#1F7A5C"
    // Trim and split copy the file's hidden details into the new files (checked with mat2); every other
    // tool leaves them out. Said in amber where they are kept, since that is the case to notice.
    readonly property bool copyKeeps: edit.tool === "trim" || edit.tool === "split"
    readonly property color keepInk: dark ? "#E0A85A" : "#8A5100"
    property bool moreFacts: false
    // What a trim cuts away: faded toward the page, not greyed (grey over the waveform looked muddy).
    readonly property color cutAway: dark ? Qt.rgba(0.02, 0.025, 0.03, 0.30) : Qt.rgba(0.35, 0.38, 0.42, 0.12)
    readonly property color accentEdge: dark ? accent : "#2A8564"

    readonly property bool timeTool: edit.tool === "trim" || edit.tool === "split" || edit.tool === "bleep"
    readonly property bool isImage: !timeTool
    // The tools that draw on the picture (for a video: on one frame of it).
    readonly property bool picTool: edit.tool === "crop" || edit.tool === "redact" || edit.tool === "annotate"
    readonly property size imgSize: isImage ? edit.imageSize() : Qt.size(0, 0)

    // ---- crop state, in picture pixels
    property real cropX: 0
    property real cropY: 0
    property real cropW: imgSize.width
    property real cropH: imgSize.height
    property real ratio: 0   // 0 = free

    function setBox(x, y, w, h) {
        w = Math.max(8, Math.min(imgSize.width, w))
        h = Math.max(8, Math.min(imgSize.height, h))
        cropX = Math.max(0, Math.min(imgSize.width - w, x))
        cropY = Math.max(0, Math.min(imgSize.height - h, y))
        cropW = w
        cropH = h
    }
    function pickRatio(r) {
        ratio = r
        if (r <= 0) return
        let w = imgSize.width, h = w / r
        if (h > imgSize.height) { h = imgSize.height; w = h * r }
        setBox((imgSize.width - w) / 2, (imgSize.height - h) / 2, w, h)
    }
    // Drag corner (0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right) to picture point (px, py).
    // Free shape only: move one side (0 top, 1 bottom, 2 left, 3 right) and keep the other three.
    function resizeEdge(side, px, py) {
        const right = cropX + cropW, bottom = cropY + cropH
        if (side === 0) { const y = Math.max(0, Math.min(bottom - 8, py)); setBox(cropX, y, cropW, bottom - y) }
        else if (side === 1) setBox(cropX, cropY, cropW, Math.max(cropY + 8, Math.min(imgSize.height, py)) - cropY)
        else if (side === 2) { const x = Math.max(0, Math.min(right - 8, px)); setBox(x, cropY, right - x, cropH) }
        else setBox(cropX, cropY, Math.max(cropX + 8, Math.min(imgSize.width, px)) - cropX, cropH)
    }
    function resizeFrom(corner, px, py) {
        const ax = (corner % 2 === 0) ? cropX + cropW : cropX     // the opposite corner stays put
        const ay = corner < 2 ? cropY + cropH : cropY
        px = Math.max(0, Math.min(imgSize.width, px))
        py = Math.max(0, Math.min(imgSize.height, py))
        let w = Math.max(8, Math.abs(px - ax)), h = Math.max(8, Math.abs(py - ay))
        const sx = px < ax ? -1 : 1, sy = py < ay ? -1 : 1
        if (ratio > 0) {
            h = w / ratio
            // keep inside the picture
            const room = (sy < 0 ? ay : imgSize.height - ay)
            if (h > room) { h = room; w = h * ratio }
            const roomX = (sx < 0 ? ax : imgSize.width - ax)
            if (w > roomX) { w = roomX; h = w / ratio }
        }
        setBox(sx < 0 ? ax - w : ax, sy < 0 ? ay - h : ay, w, h)
    }

    // ---- redact state
    ListModel { id: boxes }
    property string findNote: ""
    property string findAdvice: ""
    // The suggestions are added as ordinary boxes: the person sees each one and can undo it.
    Connections {
        target: edit
        function onPrivateFound(found, error) {
            findBtn.busy = false
            if (error.length) { win.findNote = "Couldn't read the text: " + error; return }
            const names = { email: "email", phone: "phone number", card: "card number", iban: "IBAN", upi: "UPI ID" }
            let added = 0
            const kinds = []
            for (const f of found) {
                // A little room around what text recognition boxed (4 px each side, 3 above and below), so the
                // edges of the letters are covered too; kept inside the picture.
                const x = Math.max(0, f.box[0] - 4), y = Math.max(0, f.box[1] - 3)
                const b = [x, y, Math.min(f.box[0] + f.box[2] + 4, imgSize.width) - x, Math.min(f.box[1] + f.box[3] + 3, imgSize.height) - y]
                let dup = false
                for (let i = 0; i < boxes.count; i++) {
                    const o = boxes.get(i)
                    if (Math.abs(o.bx - b[0]) < 2 && Math.abs(o.by - b[1]) < 2 && Math.abs(o.bw - b[2]) < 2) dup = true
                }
                if (dup) continue
                boxes.append({ bx: b[0], by: b[1], bw: b[2], bh: b[3] })
                added++
                if (kinds.indexOf(names[f.kind] || f.kind) < 0) kinds.push(names[f.kind] || f.kind)
            }
            // Text recognition matched a pattern; it does not know what the text is. Say "looks like".
            win.findNote = added ? added + (added === 1 ? " box" : " boxes") + " added where the text looks like: " + kinds.join(", ")
                                 : "Nothing found that looks like an email, phone or card number, UPI ID or IBAN"
            win.findAdvice = "Remove a box with its ×. Text recognition can miss text or cover too much. "
        }
    }
    property var drawing: null   // {x, y, w, h} in picture pixels while dragging

    // ---- background state
    property var swatches: ["#FFFFFF", "#F2EFE9", "#1E2128", "#4DB491", "#2F6FED",
                            "#FFD9B8-#F58A7A", "#BFD9FF-#7A8CF5", "#CFF5E3-#3FC6A0", "#2B3140-#0E1117", "none"]
    readonly property var swatchNames: ["White", "Paper", "Charcoal", "Green", "Blue", "Peach fade", "Sky fade",
                                        "Mint fade", "Night fade", "See-through"]
    property int swatch: 5
    ColorDialog {
        id: colourPicker
        title: "Background colour"
        onAccepted: {
            const c = String(selectedColor).substr(0, 7).toUpperCase()
            if (win.swatches.indexOf(c) < 0) win.swatches = win.swatches.concat([c])
            win.swatch = win.swatches.indexOf(c)
        }
    }
    property real padding: 60

    // ---- annotate state: shapes in picture pixels, in the form arka-convert annotate takes
    property var shapes: []
    property var draft: null
    property string pen: "pen"            // pen, arrow, box, highlight, text
    property string inkColor: "#E5484D"
    property int sizeIx: 1                // small, medium, large
    readonly property var inks: ["#E5484D", "#F5A524", "#FFD60A", "#4DB491", "#2F6FED", "#FFFFFF", "#14181F"]
    readonly property var inkNames: ["Red", "Orange", "Yellow", "Green", "Blue", "White", "Black"]
    readonly property real unit: Math.max(2, Math.max(imgSize.width, imgSize.height) / 220)
    readonly property int lineW: Math.max(1, Math.round(unit * [0.6, 1, 1.8][sizeIx]))
    readonly property int letterH: Math.max(8, Math.round(unit * [4, 6, 9][sizeIx]))
    function addShape(sh) { shapes = shapes.concat([sh]) }
    function undoShape() { if (shapes.length) shapes = shapes.slice(0, -1) }

    // ---- trim state
    readonly property real total: edit.duration
    function secs(t) {
        const p = String(t).trim().split(":")
        if (p.length < 1 || p.length > 3 || t.trim() === "") return NaN
        let s = 0
        for (let i = 0; i < p.length; i++) {
            if (!/^\d+(\.\d+)?$/.test(p[i].trim())) return NaN
            s = s * 60 + parseFloat(p[i])
        }
        return s
    }
    function clock(s) {
        const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), r = s % 60
        const rr = (Math.round(r * 10) / 10).toString()
        return (h > 0 ? h + ":" + String(m).padStart(2, "0") : m) + ":" + (r < 10 ? "0" : "") + rr
    }
    readonly property real tStart: secs(startField.text)
    readonly property real tEnd: secs(endField.text)
    readonly property bool trimOk: !isNaN(tStart) && !isNaN(tEnd) && tEnd > tStart && (total <= 0 || tStart < total)

    // File names and metadata come from files, so they are never read as HTML (an <img> in a photo's
    // details must not make this window fetch anything).
    function plain(t) { return String(t).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }

    function setTrim(which, t) {   // from the timeline: move one end, never past the other
        if (which === 0) startField.text = clock(Math.min(t, (isNaN(tEnd) ? total : tEnd) - 0.1))
        else endField.text = clock(Math.max(t, (isNaN(tStart) ? 0 : tStart) + 0.1))
    }

    // ---- split state: one time, inside the file
    readonly property real tSplit: secs(splitField.text)
    readonly property bool splitOk: !isNaN(tSplit) && tSplit > 0 && (total <= 0 || tSplit < total)

    // ---- bleep state: the parts to beep over
    ListModel { id: parts }
    readonly property real pFrom: secs(fromField.text)
    readonly property real pTo: secs(toField.text)
    readonly property bool partOk: !isNaN(pFrom) && !isNaN(pTo) && pTo > pFrom && (total <= 0 || pFrom < total)
    function addPart() {
        if (!partOk) return
        parts.append({ a: pFrom, b: total > 0 ? Math.min(pTo, total) : pTo })
        fromField.text = ""; toField.text = ""
    }

    // ---- video redact: optionally only between two times
    readonly property real rFrom: secs(rFromField.text)
    readonly property real rTo: secs(rToField.text)
    readonly property bool rWhole: rFromField.text.trim() === "" && rToField.text.trim() === ""
    readonly property bool rOk: rWhole || (!isNaN(rFrom) && !isNaN(rTo) && rTo > rFrom)

    readonly property bool canApply: {
        if (edit.tool === "crop") return cropW > 0 && (cropW < imgSize.width || cropH < imgSize.height)
        if (edit.tool === "redact") return boxes.count > 0 && (!edit.isVideo || rOk)
        if (edit.tool === "annotate") return shapes.length > 0
        if (edit.tool === "background") return true
        if (edit.tool === "split") return splitOk
        if (edit.tool === "bleep") return parts.count > 0
        return trimOk
    }
    function apply() {
        if (!canApply) return
        if (edit.tool === "crop") {
            edit.apply([cropX, cropY, cropW, cropH].map(v => String(Math.round(v))))
        } else if (edit.tool === "redact") {
            const a = edit.isVideo && !rWhole ? ["--between", rFromField.text.trim(), rToField.text.trim()] : []
            for (let i = 0; i < boxes.count; i++) {
                const b = boxes.get(i)
                a.push(String(Math.round(b.bx)), String(Math.round(b.by)), String(Math.round(b.bw)), String(Math.round(b.bh)))
            }
            edit.apply(a)
        } else if (edit.tool === "annotate") {
            const r = v => Math.round(v * 10) / 10
            edit.apply([JSON.stringify(shapes.map(sh => Object.assign({}, sh, { p: sh.p.map(r) })))])
        } else if (edit.tool === "background") {
            edit.apply([swatches[swatch], String(Math.round(padding))])
        } else if (edit.tool === "split") {
            edit.apply([splitField.text.trim()])
        } else if (edit.tool === "bleep") {
            const a = []
            for (let i = 0; i < parts.count; i++) a.push(parts.get(i).a.toFixed(3), parts.get(i).b.toFixed(3))
            edit.apply(a)
        } else {
            edit.apply([startField.text.trim(), endField.text.trim()])
        }
    }


    Shortcut { sequence: "Escape"; onActivated: Qt.quit() }
    Shortcut { sequence: "Ctrl+I"; enabled: !words.visible; onActivated: edit.inspectorOpen = !edit.inspectorOpen }
    // Enter applies, except while typing a time or a text (there it adds the part or the words).
    Shortcut {
        sequences: ["Return", "Enter"]
        enabled: !startField.activeFocus && !endField.activeFocus && !splitField.activeFocus && !fromField.activeFocus
                 && !toField.activeFocus && !rFromField.activeFocus && !rToField.activeFocus && !words.visible
        onActivated: win.apply()
    }
    Shortcut { sequences: [StandardKey.Undo]; enabled: edit.tool === "annotate" && !words.visible; onActivated: win.undoShape() }
    Shortcut { sequences: [StandardKey.Undo]; enabled: edit.tool === "redact" && boxes.count > 0; onActivated: boxes.remove(boxes.count - 1) }
    Shortcut { sequences: [StandardKey.Undo]; enabled: edit.tool === "bleep" && parts.count > 0 && !fromField.activeFocus && !toField.activeFocus
               onActivated: parts.remove(parts.count - 1) }

    component Btn: Rectangle {
        id: btn
        property string text
        property string iconName: ""
        property bool primary: false      // the main action: a neutral dark button, as in ArkaOS's own apps
        property bool selected: false     // a chosen option (tool, size, shape): the accent
        property bool tinted: false       // the panel's one suggested action: a light green wash, green words
        property real dot: 0              // instead of words: a dot this wide (the pen sizes)
        property bool flat: false         // a quiet text button: no edge, green words
        readonly property bool hovered: btnMouse.containsMouse
        property bool enabledState: true
        signal clicked
        implicitWidth: label.implicitWidth + 36 + (iconName.length ? 24 : 0)
        implicitHeight: 36
        radius: 8
        opacity: enabledState || primary ? 1 : 0.5
        activeFocusOnTab: enabledState
        // Flat: one solid colour, a hairline edge, and a clear ring when the keyboard is on it.
        color: primary ? (enabledState ? win.ink : win.line)
             : selected ? win.accent
             : !enabledState || (flat && !btnMouse.containsMouse) ? "transparent"
             : tinted ? Qt.rgba(win.accent.r, win.accent.g, win.accent.b, btnMouse.containsMouse ? 0.22 : 0.12)
             : (btnMouse.containsMouse ? win.line : win.panel)
        border.width: activeFocus ? 2 : 1
        border.color: activeFocus ? win.accentEdge : (primary && enabledState ? win.ink : selected ? win.accentEdge
                      : flat ? "transparent" : enabledState ? win.btnEdge : win.line)
        Keys.onSpacePressed: if (enabledState) clicked()
        Keys.onReturnPressed: if (enabledState) clicked()
        readonly property bool showsIcon: iconName.length > 0 && edit.hasIcon(iconName)
        Image {
            visible: btn.showsIcon
            anchors { verticalCenter: parent.verticalCenter; right: label.left; rightMargin: 8 }
            width: 18; height: 18; sourceSize: Qt.size(36, 36)
            source: btn.showsIcon ? "image://icon/" + btn.iconName + "?tint=" + String(label.color).substr(1) : ""
        }
        Rectangle {
            visible: btn.dot > 0
            anchors.centerIn: parent; width: btn.dot; height: btn.dot; radius: btn.dot / 2; color: label.color
        }
        Text { textFormat: Text.PlainText;
            id: label
            visible: btn.dot === 0
            anchors.centerIn: parent
            anchors.horizontalCenterOffset: btn.showsIcon ? 12 : 0
            text: btn.text
            color: btn.primary ? (btn.enabledState ? win.bg : win.muted) : btn.selected ? win.accentInk
                 : btn.tinted || btn.flat ? (win.dark ? "#6FCBA8" : "#17624A") : win.ink
            font.pixelSize: 13
            font.weight: Font.DemiBold
        }
        MouseArea {
            id: btnMouse
            anchors.fill: parent
            enabled: btn.enabledState
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: btn.clicked()
        }
        Accessible.role: Accessible.Button
        Accessible.name: text
        Accessible.onPressAction: if (enabledState) clicked()
    }

    // A tooltip in the house style: rounded, dark ink with light words (the stock one was a square box).
    component Tip: ToolTip {
        id: tip
        delay: 400
        contentItem: Text { textFormat: Text.PlainText; text: tip.text; color: win.bg; font.pixelSize: 12 }
        background: Rectangle { radius: 8; color: win.ink }
        padding: 8
    }
    component TimeField: TextField {
        id: tf
        width: 110
        implicitHeight: 32
        color: win.ink
        selectByMouse: true
        placeholderText: "0:00"
        placeholderTextColor: win.muted
        background: Rectangle { radius: 6; color: win.dark ? win.bg : "#FFFFFF"; border.width: tf.activeFocus ? 2 : 1
                                border.color: tf.activeFocus ? win.accentEdge : win.edge }
    }


    // ---- inspector: what this file is, on the right; the button at the top right hides it
    readonly property var facts: edit.details()
    // What the copy will be, as far as it is known from the choices (never an estimate of file size).
    readonly property string resultText: {
        if (edit.tool === "crop") return Math.round(cropW) + " × " + Math.round(cropH) + " px"
        if (edit.tool === "trim") return trimOk ? clock(Math.min(tEnd, total > 0 ? total : tEnd) - tStart) : ""
        if (edit.tool === "split") return splitOk && total > 0 ? clock(tSplit) + " + " + clock(total - tSplit) : ""
        if (edit.tool === "bleep") return parts.count ? parts.count + (parts.count === 1 ? " part" : " parts") : ""
        if (edit.tool === "redact") return boxes.count ? boxes.count + (boxes.count === 1 ? " box" : " boxes") : ""
        if (edit.tool === "background") return Math.round(imgSize.width + 2 * padding) + " × " + Math.round(imgSize.height + 2 * padding) + " px"
        return ""
    }
    // What the first row of the File card calls the copy-to-be.
    readonly property string resultLabel: ({ crop: "New size", background: "New size", trim: "New length", split: "Parts",
                                             bleep: "Bleeped", redact: "Covered" })[edit.tool] ?? "Result"
    Rectangle {
        id: inspectorToggle
        z: 3
        anchors { right: parent.right; top: parent.top; rightMargin: 10; topMargin: 20 }
        width: toggleLabel.implicitWidth + 50; height: 34; radius: 8
        // Flat, like a toolbar button: the accent only marks the icon while the panel is open.
        color: toggleMouse.containsMouse ? win.line : "transparent"
        border.color: inspectorToggle.activeFocus ? win.accentEdge : "transparent"
        border.width: 2
        activeFocusOnTab: true
        Keys.onSpacePressed: edit.inspectorOpen = !edit.inspectorOpen
        // a panel icon: a frame with its right part filled
        Rectangle {
            anchors { left: parent.left; leftMargin: 11; verticalCenter: parent.verticalCenter }
            width: 16; height: 13; radius: 2; color: "transparent"
            border.width: 1.5; border.color: edit.inspectorOpen ? win.accentEdge : win.muted
            Rectangle { anchors { right: parent.right; top: parent.top; bottom: parent.bottom; margins: 2 } width: 5; radius: 1
                        color: edit.inspectorOpen ? win.accentEdge : win.muted }
        }
        property bool justClicked: false
        Text { id: toggleLabel; textFormat: Text.PlainText
               anchors { left: parent.left; leftMargin: 34; verticalCenter: parent.verticalCenter }
               text: edit.inspectorOpen ? "Hide details" : "Details"   // says what a click does
               color: edit.inspectorOpen ? win.accentText : win.muted; font.pixelSize: 12; font.weight: Font.DemiBold }
        MouseArea { id: toggleMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                    onClicked: { inspectorToggle.justClicked = true; edit.inspectorOpen = !edit.inspectorOpen }
                    onExited: inspectorToggle.justClicked = false }
        Tip { visible: toggleMouse.containsMouse && !parent.justClicked; text: (edit.inspectorOpen ? "Hide" : "Show") + " details (Ctrl+I)" }
        Accessible.role: Accessible.Button
        Accessible.name: edit.inspectorOpen ? "Hide details" : "Show details"
        Accessible.onPressAction: edit.inspectorOpen = !edit.inspectorOpen
    }
    Rectangle {
        id: inspector
        anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
        width: edit.inspectorOpen ? 280 : 0
        Behavior on width { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
        clip: true
        color: win.panel
        HoverHandler { id: inspectorHover }
        Rectangle { anchors { left: parent.left; top: parent.top; bottom: parent.bottom } width: 1; color: win.line }
        // True of every edit here (arka-convert runs its tools in a sandbox with no network), so it is always
        // in view, under the scrolling part of the panel.
        Text { textFormat: Text.PlainText
            id: keepNote
            visible: false   // the card says it (the same sentence twice read as noise)
            anchors { left: parent.left; right: parent.right; bottom: panelNote.top; leftMargin: 20; rightMargin: 20; bottomMargin: 8 }
            wrapMode: Text.WordWrap; font.pixelSize: 13; font.weight: Font.DemiBold; color: win.keepInk
            text: edit.tool === "split" ? "Both new files keep this file's hidden details." : "The copy keeps this file's hidden details."
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: panelNote.top; leftMargin: 20; rightMargin: 20; bottomMargin: 12 }
                    height: 1; color: win.line }
        Text { textFormat: Text.PlainText
            id: panelNote
            // Right under the panel's content; at the bottom (and the content scrolls above it) when there is more.
            anchors { left: parent.left; right: parent.right; leftMargin: 20; rightMargin: 20 }
            y: Math.min(parent.height - height - 16,
                        inspectorScroll.y + info.height + 28 + 0)
            wrapMode: Text.WordWrap
            // What each copy carries was checked with mat2 (2026-11-06): pictures and blacked-out, cropped or bleeped
            // sound and video leave the file's details out; trim and split copy them over.
            text: (edit.tool === "split" ? "Saved as two new files next to this one; this one is not changed. "
                                         : "Saved as a new file next to this one; this one is not changed. ")
                  + "Made on this computer, by tools with no network access."
            color: win.muted; font.pixelSize: 13
        }
        Flickable {
            id: inspectorScroll
            anchors { fill: parent; topMargin: 52; bottomMargin: panelNote.height + 0 + 28 }
            contentHeight: info.height + 48
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            ScrollBar.vertical: ScrollBar {
                policy: ScrollBar.AsNeeded
                visible: parent.contentHeight > parent.height && (inspectorHover.hovered || parent.moving)
                contentItem: Rectangle { implicitWidth: 5; radius: 3; color: win.muted; opacity: 0.6 }
            }
            // A soft fade at the bottom says there is more below.
            Rectangle {
                parent: inspector
                visible: inspectorScroll.contentHeight > inspectorScroll.height && !inspectorScroll.atYEnd
                anchors { left: parent.left; right: parent.right; bottom: inspectorScroll.bottom; leftMargin: 1 }
                height: 56
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.rgba(win.panel.r, win.panel.g, win.panel.b, 0) }
                    GradientStop { position: 0.6; color: win.panel }
                }
                Text { textFormat: Text.PlainText
                    anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 8 }
                    text: "More below ⌄"; color: win.muted; font.pixelSize: 12 }
            }
            Column {
                id: info
                x: 20; width: 240
                spacing: 14
                // No picture of the file here: the main area already shows it.
                Column {
                    width: parent.width; spacing: 3
                    Text { textFormat: Text.PlainText; width: parent.width; text: edit.fileName; color: win.ink; font.pixelSize: 15; font.weight: Font.DemiBold
                           elide: Text.ElideMiddle }
                    Text { textFormat: Text.PlainText; width: parent.width; text: win.facts.kind + " · " + win.facts.format; color: win.muted; font.pixelSize: 13 }
                }
                // What the file carries besides what you see: found by mat2, removed in a copy on request.
                Text { textFormat: Text.PlainText; text: "Hidden details"; color: win.ink; font.pixelSize: 13; font.weight: Font.DemiBold }
                Rectangle {
                    width: parent.width; radius: 10; color: win.bg; border.color: win.line
                    height: hiddenCol.height + 20
                    Column {
                        id: hiddenCol
                        x: 12; y: 10; width: parent.width - 24
                        spacing: 8
                        readonly property var info: edit.hidden
                        // Read straight from the window's data: a binding on the sibling "info" was
                        // evaluated before it was set and stayed empty.
                        readonly property var groups: edit.hidden && edit.hidden.groups ? edit.hidden.groups : []
                        Text { textFormat: Text.PlainText;
                            width: parent.width; wrapMode: Text.WordWrap
                            visible: hiddenCol.groups.length === 0
                            // Only what mat2 actually checked is claimed: it can't read every detail in every format.
                            text: hiddenCol.info && hiddenCol.info.checking ? "Checking…"
                                  : !hiddenCol.info || hiddenCol.info.supported === undefined ? "These could not be checked."
                                  : hiddenCol.info.supported === false ? "ArkaOS can't check this kind of file yet."
                                  : (hiddenCol.info.total > 0
                                     ? "Nothing personal found; only how the file is stored. "
                                     : "None found in this file. ")
                                    + "Not every kind of detail can be read in every format."
                            color: win.muted; font.pixelSize: 12
                        }
                        Repeater {
                            model: hiddenCol.groups
                            delegate: Column {
                                required property var modelData
                                width: hiddenCol.width; spacing: 2
                                Row {
                                    spacing: 6
                                    readonly property bool place: modelData[0] === "Location"
                                    Rectangle {   // a mark that does not rest on colour alone: where the picture was taken
                                        visible: parent.place; width: 14; height: 14; radius: 7; anchors.verticalCenter: parent.verticalCenter
                                        color: win.dark ? "#E07A50" : "#B4481F"
                                        Text { textFormat: Text.PlainText; anchors.centerIn: parent; text: "!"; color: win.panel
                                               font.pixelSize: 10; font.weight: Font.Bold }
                                    }
                                    Text { textFormat: Text.PlainText
                                           text: parent.place ? "Location: where it was taken" : modelData[0]
                                           color: parent.place ? (win.dark ? "#E07A50" : "#B4481F") : win.ink
                                           font.pixelSize: 12; font.weight: Font.DemiBold }
                                }
                                Text { textFormat: Text.PlainText;
                                    width: parent.width; wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight
                                    color: win.muted; font.pixelSize: 12
                                    // The most telling value first, e.g. the whole position for a location.
                                    text: {
                                        const rows = modelData[1]
                                        const pos = rows.find(r => /position/i.test(r[0]))
                                        const pick = pos ? [pos] : rows.slice(0, 2)
                                        // 13 deg 4' 52.00" N → 13°4′52″ N
                                        const nice = v => String(v).replace(/(\d+) deg (\d+)' ([\d.]+)"/g,
                                                                            (m, d, mi, se) => d + "°" + mi + "′" + Math.round(Number(se)) + "″")
                                        return pick.map(r => nice(r[1])).join(" · ")
                                               + (rows.length > pick.length ? "\n" + (rows.length - pick.length) + " more in this group" : "")
                                    }
                                }
                            }
                        }
                        Text { textFormat: Text.PlainText
                            visible: hiddenCol.groups.length > 0
                            width: parent.width; wrapMode: Text.WordWrap; font.pixelSize: 12
                            text: win.copyKeeps ? "Your edited copy will keep these too."
                                                : "Your edited copy will leave these out."
                            color: win.copyKeeps ? win.keepInk : win.muted
                            font.weight: win.copyKeeps ? Font.DemiBold : Font.Normal
                        }
                        Btn {
                            id: removeHiddenBtn
                            visible: hiddenCol.groups.length > 0
                            width: parent.width
                            property bool started: false
                            text: started ? "Saving a copy without them…" : "Save a copy without them"
                            iconName: "document-cleanup"
                            flat: true
                            enabledState: !started
                            onClicked: started = edit.removeHidden()
                        }
                    }
                }
                Text { textFormat: Text.PlainText; text: "File"; color: win.ink; font.pixelSize: 13; font.weight: Font.DemiBold }
                Rectangle {
                    width: parent.width; radius: 10; color: win.bg; border.color: win.line
                    height: rows.height + 8
                    Column {
                        id: rows
                        y: 4; width: parent.width
                        Repeater {
                            model: {
                                // What the copy will be comes first, so it is never the row that scrolls out of view.
                                const rows = win.resultText.length ? [[win.resultLabel, win.resultText], ["Size", win.facts.size]] : [["Size", win.facts.size]]
                                if (win.isImage) rows.push(["Dimensions", win.imgSize.width + " × " + win.imgSize.height + " px"])
                                if (win.timeTool || edit.isVideo) rows.push(["Length", win.total > 0 ? win.clock(win.total) : "unknown"])
                                if (win.moreFacts) rows.push(["Format", win.facts.format], ["Changed", win.facts.changed], ["Folder", win.facts.folder])
                                return rows
                            }
                            delegate: Item {
                                required property var modelData
                                required property int index
                                width: rows.width; height: 34
                                Rectangle { visible: index > 0; anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 12; rightMargin: 12 }
                                            height: 1; color: win.line }
                                Text { textFormat: Text.PlainText; anchors { left: parent.left; leftMargin: 12; verticalCenter: parent.verticalCenter }
                                       text: modelData[0]; color: win.muted; font.pixelSize: 13 }
                                Text { textFormat: Text.PlainText; id: rowValue; anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                                       width: parent.width - 110; horizontalAlignment: Text.AlignRight
                                       elide: modelData[0] === "Folder" ? Text.ElideLeft : Text.ElideMiddle
                                       readonly property bool result: index === 0 && win.resultText.length > 0
                                       text: modelData[1]; color: result ? win.accentText : win.ink
                                       font.pixelSize: 13; font.weight: result ? Font.DemiBold : Font.Normal }
                                MouseArea { id: rowHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                                Tip { visible: rowHover.containsMouse && rowValue.truncated; text: modelData[1] }   // plain text
                            }
                        }
                    }
                }
                Row {   // the file itself: after its hidden details and facts, which are what this panel is for
                    width: parent.width; spacing: 8
                    Btn { text: "Open"; implicitHeight: 32; width: (parent.width - 8) / 2; onClicked: edit.openFile() }
                    Btn { text: "Show in folder"; implicitHeight: 32; width: (parent.width - 8) / 2; onClicked: edit.showInFolder() }
                }
                Text { textFormat: Text.PlainText
                    text: win.moreFacts ? "Fewer details ⌃" : "Format, date and folder ⌄"
                    color: win.accentText; font.pixelSize: 12; font.weight: Font.DemiBold
                    MouseArea { anchors { fill: parent; margins: -6 } cursorShape: Qt.PointingHandCursor; onClicked: win.moreFacts = !win.moreFacts }
                    Accessible.role: Accessible.Button
                    Accessible.name: win.moreFacts ? "Show fewer details" : "Show format, date and folder"
                    Accessible.onPressAction: win.moreFacts = !win.moreFacts
                }
            }
        }
    }

    // ---- zoom, for the picture tools: on the title's line, away from every handle on the picture
    Row {
        visible: win.picTool && win.imgSize.width > 0
        anchors { right: inspector.left; top: parent.top; rightMargin: edit.inspectorOpen ? 16 : 76; topMargin: 20 }
        spacing: 6
        // Before zooming in: the scale as words and "+" (a dimmed "−" and "Fit" looked broken; "+" alone was unclear).
        Text { textFormat: Text.PlainText; visible: stage.zoom <= 1; anchors.verticalCenter: parent.verticalCenter
               text: Math.round(stage.sc * 100) + "% of real size"; color: win.muted; font.pixelSize: 13 }
        Btn { text: "−"; implicitWidth: 36; visible: stage.zoom > 1
              onClicked: stage.zoomTo(stage.zoom / 1.5, stage.width / 2, stage.height / 2)
              Accessible.name: "Zoom out"; Tip { visible: parent.hovered; text: "Zoom out (Ctrl+−)"; y: parent.height + 6 } }
        Btn { text: Math.round(stage.sc * 100) + "%"; implicitWidth: 64; visible: stage.zoom > 1
              onClicked: stage.zoomTo(1, 0, 0)
              Accessible.name: "Show the whole picture"
              Tip { visible: parent.hovered; y: parent.height + 6
                    text: stage.zoom > 1 ? Math.round(stage.sc * 100) + "% of the picture's real size. Click for the whole picture (Ctrl+0)"
                                         : "The whole picture is shown" } }
        Btn { text: "+"; implicitWidth: 36; enabledState: stage.zoom < 8
              onClicked: stage.zoomTo(stage.zoom * 1.5, stage.width / 2, stage.height / 2)
              Accessible.name: "Zoom in"; Tip { visible: parent.hovered; text: "Zoom in (Ctrl++)"; y: parent.height + 6; x: parent.width - width } }
    }
    Shortcut { sequences: [StandardKey.ZoomIn, "Ctrl+="]; enabled: win.picTool; onActivated: stage.zoomTo(stage.zoom * 1.5, stage.width / 2, stage.height / 2) }
    Shortcut { sequences: [StandardKey.ZoomOut]; enabled: win.picTool; onActivated: stage.zoomTo(stage.zoom / 1.5, stage.width / 2, stage.height / 2) }
    Shortcut { sequence: "Ctrl+0"; enabled: win.picTool; onActivated: stage.zoomTo(1, 0, 0) }

    // ---- header, as ArkaOS's own apps have it: a serif title, then what to do in plain words
    Column {
        id: heading
        anchors { left: parent.left; right: inspector.left; top: parent.top; leftMargin: 16; rightMargin: 64; topMargin: 18 }
        spacing: 4
        bottomPadding: 6
        Text { textFormat: Text.PlainText
            width: parent.width
            text: ({ crop: edit.isVideo ? "Crop video" : "Crop", redact: "Black out", annotate: "Annotate",
                     background: "Background", trim: "Trim", split: "Split in two", bleep: "Bleep out" })[edit.tool]
            color: win.ink
            font.family: "Noto Serif"
            font.pixelSize: 26
            font.weight: Font.Medium
            elide: Text.ElideRight
            Accessible.role: Accessible.Heading
        }
        Text { textFormat: Text.PlainText
            width: parent.width
            text: ({ crop: (win.ratio > 0 ? "Drag the corners to choose what to keep." : "Drag the corners or sides to choose what to keep.")
                           + (edit.isVideo ? " The whole video is cropped the same way." : ""),
                     redact: edit.isVideo ? "Drag over anything you want covered with black, in every frame or only for a while."
                                          : "Drag over anything you want covered with black, or let Find private text suggest boxes.",
                     annotate: "Pick a tool and draw. For text, click where the words go.",
                     background: "Pick a colour and how much room to leave around the picture.",
                     trim: "Drag the ends of the timeline to the part you want to keep.",
                     split: "Click the timeline where to cut. Both halves are saved as new files.",
                     bleep: "Drag across the timeline over each part to bleep out, or type the times." })[edit.tool]
            color: win.muted
            font.pixelSize: 13
            wrapMode: Text.WordWrap
            maximumLineCount: 2
        }
    }

    // ---- picture stage (crop and redact)
    Item {
        id: stage
        visible: win.picTool
        // The stage reaches 16 px past the picture on every side, so a corner
        // handle that sits on the picture's edge is wholly inside the area that
        // takes mouse events (Qt delivers none outside an item's parent).
        anchors { left: parent.left; right: inspector.left; top: heading.bottom; bottom: controls.top; topMargin: 12 }

        clip: true      // a zoomed picture stays inside its area
        Rectangle {   // a hairline and a soft shadow at the picture's edge (a white page on a light canvas had none)
            x: stage.ox - 1; y: stage.oy - 1; width: img.width + 2; height: img.height + 2
            visible: img.status === Image.Ready
            color: "transparent"; border.color: win.dark ? "#3A404A" : "#C9CED6"; border.width: 1
            Rectangle { z: -1; x: 0; y: 2; width: parent.width; height: parent.height; radius: 2; color: Qt.rgba(0, 0, 0, win.dark ? 0.35 : 0.08) }
        }
        Image {
            id: img
            x: stage.ox; y: stage.oy
            width: win.imgSize.width * stage.sc; height: win.imgSize.height * stage.sc
            fillMode: Image.Stretch
            autoTransform: true
            smooth: true
            mipmap: true
            source: win.picTool ? edit.pictureUrl : ""
        }
        // Zoom 1 shows the whole picture, 16 px inside the area; up to 8 times that. (panX, panY) is the point
        // of the picture, as fractions, at the middle of the area; it is held so the picture fills what it can.
        property real zoom: 1
        property real panX: 0.5
        property real panY: 0.5
        readonly property real fit: win.imgSize.width > 0 && width > 32 && height > 32
                                    ? Math.min((width - 32) / win.imgSize.width, (height - 32) / win.imgSize.height) : 1
        readonly property real sc: fit * zoom
        readonly property real ox: Math.round(width / 2 - clampPan(panX, width, win.imgSize.width) * win.imgSize.width * sc)
        readonly property real oy: Math.round(height / 2 - clampPan(panY, height, win.imgSize.height) * win.imgSize.height * sc)
        function px(v) { return (v - ox) / sc }
        function py(v) { return (v - oy) / sc }
        function clampPan(p, view, size) {
            const shown = size * sc
            if (shown <= view - 32) return 0.5
            const half = (view / 2 - 16) / shown
            return Math.max(half, Math.min(1 - half, p))
        }
        // Zoom to z, keeping the point of the picture under (cx, cy) where it is.
        function zoomTo(z, cx, cy) {
            z = Math.max(1, Math.min(8, z))
            const qx = px(cx), qy = py(cy)
            zoom = z
            if (z === 1) { panX = 0.5; panY = 0.5; return }
            panX = clampPan((width / 2 - cx + qx * sc) / (win.imgSize.width * sc), width, win.imgSize.width)
            panY = clampPan((height / 2 - cy + qy * sc) / (win.imgSize.height * sc), height, win.imgSize.height)
        }
        function panBy(dx, dy) {   // in screen pixels
            panX = clampPan(panX - dx / (win.imgSize.width * sc), width, win.imgSize.width)
            panY = clampPan(panY - dy / (win.imgSize.height * sc), height, win.imgSize.height)
        }
        Timer { id: panHintTimer; interval: 4000 }
        Connections { target: stage; function onZoomChanged() { if (stage.zoom > 1) panHintTimer.restart() } }
        Rectangle {   // just after zooming: how to move around (then it gets out of the way)
            visible: stage.zoom > 1 && panHintTimer.running
            z: 5
            anchors { left: parent.left; bottom: parent.bottom; margins: 12 }
            width: panHint.implicitWidth + 20; height: 28; radius: 14; color: Qt.rgba(win.ink.r, win.ink.g, win.ink.b, 0.72)
            Text { textFormat: Text.PlainText; id: panHint; anchors.centerIn: parent; color: win.bg; font.pixelSize: 12
                   text: "Scroll to move · Ctrl + scroll to zoom · Ctrl+0 for the whole picture" }
        }
        // Ctrl + wheel zooms at the pointer; the wheel alone moves a zoomed picture; the middle button drags it.
        WheelHandler {
            acceptedModifiers: Qt.ControlModifier
            onWheel: (ev) => stage.zoomTo(stage.zoom * (ev.angleDelta.y > 0 ? 1.25 : 0.8), ev.x, ev.y)
        }
        WheelHandler {
            acceptedModifiers: Qt.NoModifier
            enabled: stage.zoom > 1
            onWheel: (ev) => stage.panBy(ev.angleDelta.x / 2, ev.angleDelta.y / 2)
        }
        DragHandler {
            acceptedButtons: Qt.MiddleButton
            target: null
            property point last: Qt.point(0, 0)
            onActiveChanged: last = Qt.point(0, 0)
            onTranslationChanged: { stage.panBy(translation.x - last.x, translation.y - last.y); last = translation }
        }

        // crop
        Item {
            id: cropLayer
            visible: edit.tool === "crop"
            anchors.fill: parent
            readonly property real bx: stage.ox + win.cropX * stage.sc
            readonly property real by: stage.oy + win.cropY * stage.sc
            readonly property real bw: win.cropW * stage.sc
            readonly property real bh: win.cropH * stage.sc
            // everything outside the box is dimmed
            Rectangle { color: "#99000000"; x: stage.ox; y: stage.oy; width: img.paintedWidth; height: cropLayer.by - stage.oy }
            Rectangle { color: "#99000000"; x: stage.ox; y: cropLayer.by + cropLayer.bh; width: img.paintedWidth; height: stage.oy + img.paintedHeight - cropLayer.by - cropLayer.bh }
            Rectangle { color: "#99000000"; x: stage.ox; y: cropLayer.by; width: cropLayer.bx - stage.ox; height: cropLayer.bh }
            Rectangle { color: "#99000000"; x: cropLayer.bx + cropLayer.bw; y: cropLayer.by; width: stage.ox + img.paintedWidth - cropLayer.bx - cropLayer.bw; height: cropLayer.bh }
            Rectangle {
                x: cropLayer.bx; y: cropLayer.by; width: cropLayer.bw; height: cropLayer.bh
                color: "transparent"; border.color: "#FFFFFF"; border.width: 2
                Rectangle { anchors { fill: parent; margins: -1 } color: "transparent"; border.color: "#FFFFFF"; border.width: 1 }
                Rectangle { anchors { fill: parent; margins: -2 } color: "transparent"; border.color: Qt.rgba(0, 0, 0, 0.45); border.width: 1 }
                // Thirds, to help place what matters.
                Repeater {
                    model: 4
                    delegate: Rectangle {
                        required property int index
                        readonly property bool vertical: index < 2
                        x: vertical ? parent.width * (index + 1) / 3 : 0
                        y: vertical ? 0 : parent.height * (index - 1) / 3
                        width: vertical ? 1 : parent.width
                        height: vertical ? parent.height : 1
                        color: Qt.rgba(1, 1, 1, 0.6)
                        Rectangle {   // a dark shadow beside each line, so it shows on a bright sky too
                            x: parent.vertical ? 1 : 0; y: parent.vertical ? 0 : 1
                            width: parent.width; height: parent.height; color: Qt.rgba(0, 0, 0, 0.22)
                        }
                    }
                }
            }
            Repeater {
                model: 4
                delegate: Rectangle {
                    required property int index
                    width: 18; height: 18; radius: 4
                    color: win.accent; border.color: win.accentInk; border.width: 1
                    // A white ring outside the dark edge, so the handle shows on any picture, dark or light.
                    Rectangle { anchors.centerIn: parent; width: parent.width + 4; height: parent.height + 4; radius: 6
                                color: "transparent"; border.color: "#FFFFFF"; border.width: 1.5
                                Rectangle { anchors.centerIn: parent; width: parent.width + 2; height: parent.height + 2; radius: 7
                                            color: "transparent"; border.color: Qt.rgba(0.08, 0.09, 0.12, 0.7); border.width: 1 } }
                    x: cropLayer.bx + (index % 2 === 0 ? 0 : cropLayer.bw) - width / 2
                    y: cropLayer.by + (index < 2 ? 0 : cropLayer.bh) - height / 2
                }
            }
            // The middle of each side (Free shape only): top, bottom, left, right.
            Repeater {
                model: win.ratio > 0 ? 0 : 4
                delegate: Rectangle {
                    required property int index
                    readonly property bool across: index < 2
                    width: across ? 36 : 10; height: across ? 10 : 36; radius: 5
                    color: win.accent; border.color: win.accentInk; border.width: 1
                    Rectangle { anchors.centerIn: parent; width: parent.width + 4; height: parent.height + 4; radius: 7
                                color: "transparent"; border.color: "#FFFFFF"; border.width: 1.5
                                Rectangle { anchors.centerIn: parent; width: parent.width + 2; height: parent.height + 2; radius: 8
                                            color: "transparent"; border.color: Qt.rgba(0.08, 0.09, 0.12, 0.7); border.width: 1 } }
                    x: across ? cropLayer.bx + cropLayer.bw / 2 - width / 2 : cropLayer.bx + (index === 2 ? 0 : cropLayer.bw) - width / 2
                    y: across ? cropLayer.by + (index === 0 ? 0 : cropLayer.bh) - height / 2 : cropLayer.by + cropLayer.bh / 2 - height / 2
                }
            }
            // One mouse area does the work, so a drag is not lost when the box moves under it.
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                property int mode: -1       // 0-3 a corner, 4 the whole box, 5-8 a side (top, bottom, left, right), -1 nothing
                property real gx: 0
                property real gy: 0
                function hit(x, y) {
                    for (let i = 0; i < 4; i++) {
                        const cx = cropLayer.bx + (i % 2 === 0 ? 0 : cropLayer.bw)
                        const cy = cropLayer.by + (i < 2 ? 0 : cropLayer.bh)
                        if (Math.abs(x - cx) <= 18 && Math.abs(y - cy) <= 18) return i
                    }
                    if (win.ratio <= 0) {
                        const inX = x > cropLayer.bx && x < cropLayer.bx + cropLayer.bw
                        const inY = y > cropLayer.by && y < cropLayer.by + cropLayer.bh
                        if (inX && Math.abs(y - cropLayer.by) <= 12) return 5
                        if (inX && Math.abs(y - cropLayer.by - cropLayer.bh) <= 12) return 6
                        if (inY && Math.abs(x - cropLayer.bx) <= 12) return 7
                        if (inY && Math.abs(x - cropLayer.bx - cropLayer.bw) <= 12) return 8
                    }
                    if (x >= cropLayer.bx && x <= cropLayer.bx + cropLayer.bw && y >= cropLayer.by && y <= cropLayer.by + cropLayer.bh) return 4
                    return -1
                }
                onPressed: (m) => {
                    const p = mapToItem(cropLayer, m.x, m.y)
                    mode = hit(p.x, p.y)
                    if (mode === 4) { gx = stage.px(p.x) - win.cropX; gy = stage.py(p.y) - win.cropY }
                }
                onReleased: mode = -1
                onPositionChanged: (m) => {
                    const p = mapToItem(cropLayer, m.x, m.y)
                    if (!pressed) {
                        const h = hit(p.x, p.y)
                        cursorShape = h === 0 || h === 3 ? Qt.SizeFDiagCursor : h === 1 || h === 2 ? Qt.SizeBDiagCursor
                                      : h === 5 || h === 6 ? Qt.SizeVerCursor : h === 7 || h === 8 ? Qt.SizeHorCursor
                                      : h === 4 ? Qt.OpenHandCursor : Qt.ArrowCursor
                        return
                    }
                    if (mode >= 0 && mode < 4) win.resizeFrom(mode, stage.px(p.x), stage.py(p.y))
                    else if (mode === 4) win.setBox(stage.px(p.x) - gx, stage.py(p.y) - gy, win.cropW, win.cropH)
                    else if (mode >= 5) win.resizeEdge(mode - 5, stage.px(p.x), stage.py(p.y))
                }
            }
        }

        // redact
        Item {
            id: redactLayer
            visible: edit.tool === "redact"
            anchors.fill: parent
            Repeater {
                model: boxes
                delegate: Rectangle {
                    required property real bx
                    required property real by
                    required property real bw
                    required property real bh
                    x: stage.ox + bx * stage.sc; y: stage.oy + by * stage.sc
                    width: bw * stage.sc; height: bh * stage.sc
                    color: "black"
                    border.color: win.accent; border.width: 2
                }
            }
            Rectangle {
                visible: win.drawing !== null
                x: win.drawing ? stage.ox + win.drawing.x * stage.sc : 0
                y: win.drawing ? stage.oy + win.drawing.y * stage.sc : 0
                width: win.drawing ? win.drawing.w * stage.sc : 0
                height: win.drawing ? win.drawing.h * stage.sc : 0
                color: "#99000000"; border.color: win.accent; border.width: 2
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.CrossCursor
                property real sx: 0
                property real sy: 0
                function clampX(v) { return Math.max(0, Math.min(win.imgSize.width, stage.px(v))) }
                function clampY(v) { return Math.max(0, Math.min(win.imgSize.height, stage.py(v))) }
                onPressed: (m) => { sx = clampX(m.x); sy = clampY(m.y); win.drawing = { x: sx, y: sy, w: 0, h: 0 } }
                onPositionChanged: (m) => {
                    if (!pressed) return
                    const ex = clampX(m.x), ey = clampY(m.y)
                    win.drawing = { x: Math.min(sx, ex), y: Math.min(sy, ey), w: Math.abs(ex - sx), h: Math.abs(ey - sy) }
                }
                onReleased: {
                    const d = win.drawing
                    win.drawing = null
                    if (d && d.w >= 4 && d.h >= 4) boxes.append({ bx: d.x, by: d.y, bw: d.w, bh: d.h })
                }
            }
            // A remove button on each box's top right corner, above the drawing area so it gets the click.
            Repeater {
                model: boxes
                delegate: Rectangle {
                    required property real bx
                    required property real by
                    required property real bw
                    required property int index
                    required property real bh
                    // Inside the box at its right end, so it never covers the text beside it.
                    readonly property bool roomy: bh * stage.sc >= 34
                    x: stage.ox + (bx + bw) * stage.sc - width - 4
                    y: roomy ? stage.oy + by * stage.sc + 4 : stage.oy + (by + bh / 2) * stage.sc - height / 2
                    width: 24; height: 24; radius: 12
                    color: removeMouse.containsMouse ? "#FFFFFF" : "#F1F3F5"
                    border.color: "#14181F"; border.width: 1
                    Text { textFormat: Text.PlainText; anchors.centerIn: parent; anchors.verticalCenterOffset: -1
                           text: "×"; color: "#14181F"; font.pixelSize: 15; font.weight: Font.DemiBold }
                    MouseArea { id: removeMouse; anchors.centerIn: parent; width: 32; height: 32; hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor; onClicked: boxes.remove(index) }
                    Accessible.role: Accessible.Button
                    Accessible.name: "Remove box " + (index + 1)
                    Accessible.onPressAction: boxes.remove(index)
                }
            }
        }

        // annotate: the shapes are drawn here exactly as arka-convert will draw them
        Item {
            id: annoLayer
            visible: edit.tool === "annotate"
            anchors.fill: parent
            Canvas {
                id: canvas
                anchors.fill: parent
                property var all: win.draft ? win.shapes.concat([win.draft]) : win.shapes
                onAllChanged: requestPaint()
                readonly property real view: stage.sc + stage.ox * 0.001 + stage.oy * 0.000001
                onViewChanged: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d")
                    ctx.reset()
                    const sc = stage.sc, X = v => stage.ox + v * sc, Y = v => stage.oy + v * sc
                    for (const sh of all) {
                        const w = Math.max(1, sh.w * sc), p = sh.p
                        ctx.lineCap = "round"; ctx.lineJoin = "round"
                        ctx.strokeStyle = sh.c; ctx.fillStyle = sh.c; ctx.lineWidth = w
                        if (sh.t === "highlight") {
                            ctx.globalAlpha = 96 / 255
                            ctx.fillRect(X(p[0]), Y(p[1]), p[2] * sc, p[3] * sc)
                            ctx.globalAlpha = 1
                        } else if (sh.t === "box") {
                            ctx.lineCap = "butt"; ctx.lineJoin = "miter"
                            ctx.strokeRect(X(p[0]) + w / 2, Y(p[1]) + w / 2, p[2] * sc - w, p[3] * sc - w)
                        } else if (sh.t === "pen") {
                            ctx.beginPath()
                            ctx.moveTo(X(p[0]), Y(p[1]))
                            for (let i = 2; i < p.length; i += 2) ctx.lineTo(X(p[i]), Y(p[i + 1]))
                            if (p.length === 2) ctx.lineTo(X(p[0]) + 0.01, Y(p[1]))
                            ctx.stroke()
                        } else if (sh.t === "arrow") {
                            const ang = Math.atan2(p[3] - p[1], p[2] - p[0]), head = (sh.w * 3 + 8) * sc
                            ctx.lineCap = "butt"
                            ctx.beginPath()
                            ctx.moveTo(X(p[0]), Y(p[1]))
                            ctx.lineTo(X(p[2]) - head * 0.8 * Math.cos(ang), Y(p[3]) - head * 0.8 * Math.sin(ang))
                            ctx.stroke()
                            ctx.beginPath()
                            ctx.moveTo(X(p[2]), Y(p[3]))
                            ctx.lineTo(X(p[2]) - head * Math.cos(ang - 0.45), Y(p[3]) - head * Math.sin(ang - 0.45))
                            ctx.lineTo(X(p[2]) - head * Math.cos(ang + 0.45), Y(p[3]) - head * Math.sin(ang + 0.45))
                            ctx.closePath()
                            ctx.fill()
                        } else if (sh.t === "text") {
                            ctx.font = "bold " + Math.max(4, Math.round(sh.w * sc)) + "px sans-serif"
                            ctx.textBaseline = "top"
                            const light = parseInt(sh.c.substr(1, 2), 16) + parseInt(sh.c.substr(3, 2), 16) + parseInt(sh.c.substr(5, 2), 16) > 382
                            ctx.lineWidth = Math.max(1, Math.floor(sh.w / 14)) * sc * 2
                            ctx.strokeStyle = light ? "#000000" : "#FFFFFF"
                            ctx.strokeText(sh.s, X(p[0]), Y(p[1]))
                            ctx.fillText(sh.s, X(p[0]), Y(p[1]))
                        }
                    }
                }
            }
            // Where the words go while they are typed.
            TextField {
                id: words
                visible: false
                property real px: 0
                property real py: 0
                x: stage.ox + px * stage.sc
                y: stage.oy + py * stage.sc - 4
                width: Math.max(160, contentWidth + 24)
                font.pixelSize: Math.max(12, win.letterH * stage.sc)
                font.weight: Font.Bold
                color: win.inkColor
                placeholderText: "Type, then Enter"
                background: Rectangle { color: Qt.rgba(0, 0, 0, 0.35); radius: 4; border.color: win.accent }
                Accessible.name: "Words to add"
                function finish(keep) {
                    if (keep && text.trim().length)
                        win.addShape({ t: "text", c: win.inkColor, w: win.letterH, p: [px, py], s: text.trim().slice(0, 300) })
                    text = ""; visible = false
                }
                onAccepted: finish(true)
                Keys.onEscapePressed: finish(false)
                onActiveFocusChanged: if (!activeFocus && visible) finish(true)
            }
            MouseArea {
                anchors.fill: parent
                enabled: !words.visible
                cursorShape: win.pen === "text" ? Qt.IBeamCursor : Qt.CrossCursor
                property real sx: 0
                property real sy: 0
                function cx(v) { return Math.max(0, Math.min(win.imgSize.width, stage.px(v))) }
                function cy(v) { return Math.max(0, Math.min(win.imgSize.height, stage.py(v))) }
                onPressed: (m) => {
                    sx = cx(m.x); sy = cy(m.y)
                    if (win.pen === "text") {
                        words.px = sx; words.py = sy; words.visible = true; words.forceActiveFocus()
                        return
                    }
                    const w = win.pen === "highlight" ? 1 : win.lineW
                    win.draft = { t: win.pen, c: win.inkColor, w: w, p: win.pen === "pen" ? [sx, sy] : [sx, sy, sx, sy] }
                }
                onPositionChanged: (m) => {
                    if (!pressed || !win.draft) return
                    const ex = cx(m.x), ey = cy(m.y), d = win.draft
                    if (d.t === "pen") {
                        const lx = d.p[d.p.length - 2], ly = d.p[d.p.length - 1]
                        if (Math.hypot(ex - lx, ey - ly) * stage.sc < 2) return
                        win.draft = Object.assign({}, d, { p: d.p.concat([ex, ey]) })
                    } else if (d.t === "arrow") {
                        win.draft = Object.assign({}, d, { p: [sx, sy, ex, ey] })
                    } else {
                        win.draft = Object.assign({}, d, { p: [Math.min(sx, ex), Math.min(sy, ey), Math.abs(ex - sx), Math.abs(ey - sy)] })
                    }
                }
                onReleased: {
                    const d = win.draft
                    win.draft = null
                    if (!d) return
                    const ok = d.t === "pen" ? d.p.length >= 2
                             : d.t === "arrow" ? Math.hypot(d.p[2] - d.p[0], d.p[3] - d.p[1]) >= 4
                             : d.p[2] >= 2 && d.p[3] >= 2
                    if (ok) win.addShape(d)
                }
            }
        }
    }

    // ---- background preview
    Item {
        id: bgStage
        visible: edit.tool === "background"
        anchors { left: parent.left; right: inspector.left; top: heading.bottom; bottom: controls.top; margins: 16 }
        readonly property real full: Math.max(1, win.imgSize.width + 2 * win.padding)
        readonly property real fullH: Math.max(1, win.imgSize.height + 2 * win.padding)
        readonly property real s: Math.min(width / full, height / fullH)
        // See-through padding is previewed on a neutral grey; the saved copy has no colour there at all.
        readonly property var cols: win.swatches[win.swatch] === "none" ? ["#DCE0E5", "#DCE0E5"] : win.swatches[win.swatch].split("-")
        Rectangle {
            anchors.centerIn: parent
            width: bgStage.full * bgStage.s
            height: bgStage.fullH * bgStage.s
            border.color: win.line
            gradient: Gradient {
                GradientStop { position: 0; color: bgStage.cols[0] }
                GradientStop { position: 1; color: bgStage.cols[bgStage.cols.length - 1] }
            }
            Image {
                x: win.padding * bgStage.s
                y: win.padding * bgStage.s
                width: win.imgSize.width * bgStage.s
                height: win.imgSize.height * bgStage.s
                autoTransform: true
                smooth: true
                source: edit.tool === "background" ? edit.fileUrl : ""
            }
        }
    }

    // ---- trim body
    Item {
        id: trimStage
        visible: win.timeTool
        anchors { left: parent.left; right: inspector.left; top: heading.bottom; bottom: controls.top; margins: 16 }
        Column {
            id: timeCol
            anchors { top: parent.top; horizontalCenter: parent.horizontalCenter; topMargin: 8 }
            width: Math.min(parent.width, 900)
            spacing: 12
            Text { textFormat: Text.PlainText;
                visible: !edit.inspectorOpen     // the side panel already shows it
                text: edit.fileName
                color: win.ink
                font.pixelSize: 16
                font.weight: Font.DemiBold
                elide: Text.ElideMiddle
                width: parent.width
            }
            // A video shows here (a frame until it plays); sound has only the timeline.
            Rectangle {   // the video's screen: dark and the column's width, the video in its own shape in the middle
                id: videoBox
                width: parent.width
                height: edit.isVideo ? Math.max(100, Math.min(280, (trimStage.height - 140) * 0.55)) : 0
                visible: edit.isVideo
                radius: 8; color: "#0B0D10"
                Item {
                    anchors.centerIn: parent
                    height: parent.height
                    width: frame.implicitHeight > 0 ? Math.min(parent.width, height * frame.implicitWidth / frame.implicitHeight)
                                                    : parent.width
                    Image { id: frame; anchors.fill: parent; fillMode: Image.PreserveAspectFit; source: edit.hasFrame ? edit.pictureUrl : ""
                            visible: !(player.item && player.item.playing) }
                    // Only the sound and video tools play; pictures never load Qt Multimedia.
                    Loader { id: player; anchors.fill: parent; active: win.timeTool; source: "Player.qml"
                             onLoaded: item.source = edit.fileUrl }
                }
            }
            Row {
                spacing: 12
                Btn {
                    id: playBtn
                    visible: player.status === Loader.Ready && win.total > 0
                    iconName: player.item && player.item.playing ? "media-playback-pause" : "media-playback-start"
                    text: player.item && player.item.playing ? "Stop" : ({ trim: "Play the part kept", split: "Play 3 s around the cut",
                                                                           bleep: "Play the original" })[edit.tool] ?? "Play"
                    onClicked: {
                        if (edit.tool === "trim") player.item.toggle(win.tStart, Math.min(win.tEnd, win.total))
                        else if (edit.tool === "split") player.item.toggle(Math.max(0, win.tSplit - 3), Math.min(win.total, win.tSplit + 3))
                        else player.item.toggle(0, -1)
                    }
                }
                Text { textFormat: Text.PlainText
                    anchors.verticalCenter: parent.verticalCenter
                    text: (win.total > 0 ? "Length " + win.clock(win.total) : "Length unknown")
                          + (player.item && player.item.playing ? "  ·  playing " + win.clock(player.item.position) : "")
                          + (player.item && player.item.error ? "  ·  can't play it here: " + player.item.error : "")
                    color: win.muted
                    font.pixelSize: 13
                }
            }
            // The timeline: the sound drawn as a waveform. Trim: drag either end. Split: click where to cut.
            // Bleep: drag across a part. It and the time fields below always show the same times.
            Rectangle {
                id: bar
                // As tall as the window allows, so the waveform is easy to read and to aim at.
                // What is left under the video preview (if any) and the Play row, never under the footer.
                // Play row 36, the times under it 16, three 12 px gaps and the 8 px top margin: 92. Then the file's name
                // (when the side panel is hidden) and the video preview, when there is one.
                width: parent.width; radius: 10
                height: Math.max(96, trimStage.height - 92 - (edit.inspectorOpen ? 0 : 34) - (edit.isVideo ? videoBox.height + 12 : 0))
                color: win.dark ? "#16191E" : "#E7EAEE"
                border.width: activeFocus ? 2 : 1
                // The focus ring is for the keyboard: after a drag with the mouse it read as a selection.
                property bool byMouse: false
                onActiveFocusChanged: if (!activeFocus) byMouse = false
                border.color: activeFocus && !byMouse ? win.accentEdge : win.line
                clip: true
                // The keyboard way: Left/Right move the cut, or the start of a trim; Shift moves the end of a
                // trim; Ctrl takes whole seconds. Bleep parts are typed in the fields below.
                activeFocusOnTab: live && edit.tool !== "bleep"
                Accessible.role: Accessible.Slider
                Accessible.name: edit.tool === "trim" ? "Part to keep" : edit.tool === "split" ? "Where to cut" : "Timeline"
                Accessible.description: edit.tool === "trim" ? "From " + win.clock(win.tStart) + " to " + win.clock(win.tEnd)
                                      : edit.tool === "split" ? "Cut at " + win.clock(win.tSplit) : "Drag across a part to bleep it out"
                function nudge(ev, dir) {
                    byMouse = false   // the keyboard is in use: show where focus is
                    const step = (ev.modifiers & Qt.ControlModifier) ? 1 : 0.1
                    if (edit.tool === "split") splitField.text = win.clock(Math.max(0.1, Math.min(span - 0.1, (isNaN(win.tSplit) ? span / 2 : win.tSplit) + dir * step)))
                    else if (edit.tool === "trim") {
                        const end = (ev.modifiers & Qt.ShiftModifier) !== 0
                        win.setTrim(end ? 1 : 0, (end ? Math.min(win.tEnd, span) : win.tStart) + dir * step)
                    }
                    ev.accepted = true
                }
                Keys.onLeftPressed: (ev) => nudge(ev, -1)
                Keys.onRightPressed: (ev) => nudge(ev, 1)
                readonly property real span: win.total > 0 ? win.total : 1
                readonly property bool live: win.total > 0
                function xAt(t) { return width * Math.max(0, Math.min(1, t / span)) }
                function tAt(x) { return Math.max(0, Math.min(span, x / width * span)) }
                Image {
                    id: wave
                    anchors { fill: parent; margins: 8 }
                    source: edit.waveUrl
                    fillMode: Image.Stretch
                    smooth: true
                    opacity: edit.tool === "trim" && win.trimOk && bar.live ? 0.45 : (win.dark ? 0.6 : 0.7)
                }
                Item {   // trim: the part kept, drawn again at full strength over the faded whole
                    visible: edit.tool === "trim" && win.trimOk && bar.live
                    clip: true
                    x: Math.max(wave.x, bar.xAt(win.tStart)); y: wave.y; height: wave.height
                    width: Math.max(0, Math.min(wave.x + wave.width, bar.xAt(Math.min(win.tEnd, bar.span))) - x)
                    Image { x: wave.x - parent.x; width: wave.width; height: wave.height; source: edit.waveUrl
                            fillMode: Image.Stretch; smooth: true; opacity: win.dark ? 0.8 : 0.85 }
                }
                Text { textFormat: Text.PlainText;
                    visible: edit.waveUrl.toString() === ""
                    anchors.centerIn: parent
                    text: bar.live ? "No sound to draw" : "The length could not be read, so type the times below"
                    color: win.muted; font.pixelSize: 12
                }
                // trim: dim what is cut away, mark the two ends
                Rectangle { visible: edit.tool === "trim" && win.trimOk && bar.live; x: 0; width: bar.xAt(win.tStart); height: parent.height
                            color: win.cutAway }
                Rectangle { visible: edit.tool === "trim" && win.trimOk && bar.live; x: bar.xAt(Math.min(win.tEnd, bar.span))
                            width: parent.width - x; height: parent.height; color: win.cutAway }
                Rectangle {   // what is kept: a light green wash
                    visible: edit.tool === "trim" && win.trimOk && bar.live
                    x: bar.xAt(win.tStart); width: bar.xAt(Math.min(win.tEnd, bar.span)) - x; height: parent.height
                    color: Qt.rgba(win.accent.r, win.accent.g, win.accent.b, win.dark ? 0.10 : 0.12)
                }
                Repeater {
                    model: edit.tool === "trim" && win.trimOk && bar.live ? [win.tStart, Math.min(win.tEnd, bar.span)] : []
                    delegate: Item {
                        required property real modelData
                        required property int index
                        // The line at the time; the grip beside it, on the side that is kept.
                        // Kept 6 px inside the frame at either end, so a grip never sits on the frame's own edge.
                        x: Math.max(6, Math.min(bar.width - 22, bar.xAt(modelData) - (index === 0 ? 0 : 16))); width: 16; height: bar.height
                        Rectangle { x: index === 0 ? 0 : 14; y: 4; width: 2; height: parent.height - 8; color: win.accentEdge }
                        Rectangle {
                            width: 16; height: 48; radius: 4; anchors.verticalCenter: parent.verticalCenter
                            color: win.accent; border.color: win.accentInk; border.width: 1.5
                            Column {   // two grip lines
                                anchors.centerIn: parent; spacing: 3
                                Repeater { model: 2; Rectangle { width: 2; height: 14; radius: 1; color: win.accentInk; opacity: 0.55 } }
                            }
                        }
                    }
                }
                // where playback is
                Rectangle {
                    visible: player.item !== null && player.item.playing
                    x: bar.xAt(player.item ? player.item.position : 0) - 1; width: 2; height: parent.height
                    color: win.ink; opacity: 0.8
                }
                // split: the cut
                Rectangle {
                    visible: edit.tool === "split" && win.splitOk && bar.live
                    x: bar.xAt(win.tSplit) - 1.5; width: 3; height: parent.height; color: win.accent
                    Rectangle {   // a grip, so the cut looks like something to drag
                        anchors.centerIn: parent; width: 16; height: 48; radius: 4; color: win.accent
                        border.color: win.accentInk; border.width: 1
                        Column { anchors.centerIn: parent; spacing: 3
                                 Repeater { model: 2; Rectangle { width: 2; height: 14; radius: 1; color: win.accentInk; opacity: 0.55 } } }
                    }
                }
                // bleep: the parts, and the one being drawn
                Repeater {
                    model: edit.tool === "bleep" && bar.live ? parts : null
                    delegate: Rectangle {
                        required property real a
                        required property real b
                        x: bar.xAt(a); width: Math.max(3, bar.xAt(b) - bar.xAt(a)); height: bar.height
                        color: Qt.rgba(229 / 255, 72 / 255, 77 / 255, win.dark ? 0.35 : 0.28); border.color: win.dark ? "#E5484D" : "#C9272D"; border.width: 2
                        Text { textFormat: Text.PlainText; visible: parent.width > implicitWidth + 12
                               anchors { top: parent.top; topMargin: 6; horizontalCenter: parent.horizontalCenter }
                               text: win.clock(a) + " – " + win.clock(b); color: win.ink; font.pixelSize: 12; font.weight: Font.DemiBold }
                    }
                }
                Rectangle {
                    id: drawingPart
                    property real a: 0
                    property real b: 0
                    visible: false
                    x: bar.xAt(Math.min(a, b)); width: Math.max(2, Math.abs(bar.xAt(b) - bar.xAt(a))); height: bar.height
                    color: Qt.rgba(229 / 255, 72 / 255, 77 / 255, 0.25); border.color: "#E5484D"
                }
                MouseArea {
                    anchors.fill: parent
                    enabled: bar.live
                    hoverEnabled: true
                    cursorShape: edit.tool === "trim" ? Qt.SizeHorCursor : Qt.CrossCursor
                    property int grip: -1    // trim: 0 the start, 1 the end
                    onPressed: (m) => {
                        bar.byMouse = true
                        bar.forceActiveFocus()
                        const t = bar.tAt(m.x)
                        if (edit.tool === "trim") {
                            grip = Math.abs(t - win.tStart) <= Math.abs(t - Math.min(win.tEnd, bar.span)) ? 0 : 1
                            win.setTrim(grip, t)
                        } else if (edit.tool === "split") {
                            splitField.text = win.clock(t)
                        } else if (edit.tool === "bleep") {
                            drawingPart.a = t; drawingPart.b = t; drawingPart.visible = true
                        }
                    }
                    onPositionChanged: (m) => {
                        if (!pressed) return
                        const t = bar.tAt(m.x)
                        if (edit.tool === "trim") win.setTrim(grip, t)
                        else if (edit.tool === "split") splitField.text = win.clock(t)
                        else if (edit.tool === "bleep") drawingPart.b = t
                    }
                    onReleased: {
                        if (edit.tool === "bleep" && drawingPart.visible) {
                            const a = Math.min(drawingPart.a, drawingPart.b), b = Math.max(drawingPart.a, drawingPart.b)
                            drawingPart.visible = false
                            if (b - a >= 0.1) parts.append({ a: a, b: b })
                        }
                        grip = -1
                    }
                }
            }
            Item {   // the time along the timeline: the ends, and a mark every few seconds between
                width: parent.width; height: 16
                // A step that gives about six marks: 1, 2, 5, 10, 15, 30 s, 1, 2, 5, 10 min.
                readonly property real step: win.total > 0 ? [1, 2, 5, 10, 15, 30, 60, 120, 300, 600].find(v => win.total / v <= 7) || 900 : 0
                Repeater {
                    model: parent.step > 0 ? Math.floor((win.total - 0.001) / parent.step) : 0
                    delegate: Item {
                        required property int index
                        readonly property real t: (index + 1) * parent.step
                        x: bar.xAt(t); visible: x > 40 && x < parent.width - 40
                        Rectangle { x: -0.5; y: -6; width: 1; height: 5; color: win.edge }
                        Text { textFormat: Text.PlainText; anchors.horizontalCenter: parent.left; text: win.clock(parent.t)
                               color: win.muted; font.pixelSize: 12 }
                    }
                }
                Text { textFormat: Text.PlainText; text: "0:00"; color: win.muted; font.pixelSize: 12 }
                Text { textFormat: Text.PlainText; anchors.right: parent.right; text: win.total > 0 ? win.clock(win.total) : ""; color: win.muted; font.pixelSize: 12 }
            }
        }
    }

    // ---- controls
    Rectangle {
        id: controls
        anchors { left: parent.left; right: inspector.left; bottom: parent.bottom }
        // The tool's own controls on top, then a footer row with Cancel and the main button.
        readonly property int footer: 64
        height: footer + (edit.tool === "annotate" ? annotSwatches.y + annotSwatches.height + 16 : 0) + (({ background: 116, trim: 100, split: 96, bleep: 140, annotate: 0, crop: 100,
                             // Black out grows with its note, which says more once Find private text has run.
                             redact: Math.max(104, 58 + redactAdvice.height + (edit.isVideo ? 12 + 36 : 0) + 16) })[edit.tool] ?? 104)
        color: win.panel
        Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: win.line }

        // crop: shapes and size
        Row {
            visible: edit.tool === "crop"
            anchors { left: parent.left; top: parent.top; margins: 16 }
            spacing: 8
            Repeater {
                model: [["Free", 0], ["1:1", 1], ["16:9", 16 / 9], ["3:2", 3 / 2], ["4:3", 4 / 3], ["3:4", 3 / 4]]
                delegate: Btn {
                    required property var modelData
                    text: modelData[0]
                    selected: Math.abs(win.ratio - modelData[1]) < 0.001
                    onClicked: win.pickRatio(modelData[1])
                }
            }
            Item { width: 8; height: 1 }
            Btn {   // back to the whole picture, Free shape
                text: "Reset"
                enabledState: win.ratio !== 0 || win.cropX > 0 || win.cropY > 0
                              || win.cropW < win.imgSize.width || win.cropH < win.imgSize.height
                onClicked: { win.ratio = 0; win.setBox(0, 0, win.imgSize.width, win.imgSize.height) }
            }
        }
        Text { textFormat: Text.PlainText;
            visible: edit.tool === "crop"
            anchors { left: parent.left; top: parent.top; leftMargin: 16; topMargin: 62 }
            text: Math.round(win.cropW) + " × " + Math.round(win.cropH) + " px, of " + win.imgSize.width + " × " + win.imgSize.height
            color: win.muted
            font.pixelSize: 13
        }

        // redact: count and undo
        Row {
            visible: edit.tool === "redact"
            anchors { left: parent.left; top: parent.top; margins: 16 }
            spacing: 12
            Btn {
                id: findBtn
                property bool busy: false
                text: busy ? "Reading the text…" : "Find private text"
                iconName: "document-scan"
                enabledState: !busy
                onClicked: { busy = true; win.findNote = ""; edit.findPrivate() }
            }
            Btn { text: "Undo last"; enabledState: boxes.count > 0; onClicked: boxes.remove(boxes.count - 1) }
            Btn { text: "Clear all"; flat: true; enabledState: boxes.count > 0; onClicked: boxes.clear() }
            Text { textFormat: Text.PlainText;
                anchors.verticalCenter: parent.verticalCenter
                text: boxes.count === 0 ? "Nothing covered yet" : boxes.count + (boxes.count === 1 ? " box" : " boxes") + " will be covered"
                color: win.muted
                font.pixelSize: 13
            }
        }
        Text { textFormat: Text.PlainText;
            id: redactAdvice
            visible: edit.tool === "redact"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 58 }
            // What Find private text found comes first, on a line that can wrap (beside the buttons it ran
            // into the side panel).
            text: (win.findNote.length ? win.findNote + ". " : "") + win.findAdvice + (edit.isVideo
                  ? "The boxes are painted into the copy's frames, so the picture under them is gone from it; the title and other details are removed too.\nThe sound is kept as it is: use Bleep for words. The original is not changed."
                  : (win.findNote.length ? "" : "Find private text looks for email addresses, phone and card numbers, UPI IDs and IBANs; not names or addresses. ")
                    + "In the copy, the covered pixels are replaced.")
            color: win.muted
            font.pixelSize: 13
            wrapMode: Text.WordWrap
        }

        // background: swatches and padding
        Row {
            visible: edit.tool === "background"
            anchors { left: parent.left; top: parent.top; margins: 16 }
            spacing: 10
            Repeater {
                model: win.swatches
                delegate: Rectangle {
                    required property string modelData
                    required property int index
                    width: 34; height: 34; radius: 17
                    readonly property var c: modelData === "none" ? ["#FFFFFF", "#D5D9DE"] : modelData.split("-")
                    // See-through: a small checkerboard, as image programs show transparency.
                    Grid {
                        anchors.centerIn: parent; columns: 3; visible: modelData === "none"
                        Repeater { model: 9; Rectangle { required property int index; width: 6; height: 6
                                   color: index % 2 ? "#C7CCD2" : "#FFFFFF" } }
                    }
                    border.width: win.swatch === index ? 3 : 1
                    border.color: win.swatch === index ? win.accentEdge : win.edge
                    gradient: Gradient {
                        GradientStop { position: 0; color: c[0] }
                        GradientStop { position: 1; color: c[c.length - 1] }
                    }
                    MouseArea { id: swatchMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                                onClicked: win.swatch = index }
                    Tip { visible: swatchMouse.containsMouse; text: win.swatchNames[index] || modelData }
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: win.swatchNames[index] || ("Colour " + modelData)
                    Accessible.checked: win.swatch === index
                    Accessible.onPressAction: win.swatch = index
                }
            }
            Btn { text: "Custom…"; onClicked: colourPicker.open(); Accessible.name: "Choose your own colour" }
            Text { textFormat: Text.PlainText; anchors.verticalCenter: parent.verticalCenter
                   text: win.swatchNames[win.swatch] || win.swatches[win.swatch]; color: win.muted; font.pixelSize: 13 }
        }
        Row {
            visible: edit.tool === "background"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 62 }
            spacing: 12
            Text { textFormat: Text.PlainText; text: "Room around it"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            Slider {
                id: padSlider
                width: parent.width - 200
                from: 0; to: 300; value: win.padding
                onMoved: win.padding = value
                Accessible.name: "Room around the picture"
                background: Rectangle {
                    x: padSlider.leftPadding
                    y: padSlider.topPadding + padSlider.availableHeight / 2 - height / 2
                    width: padSlider.availableWidth; height: 4; radius: 2
                    color: win.edge
                    Rectangle { width: padSlider.visualPosition * parent.width; height: parent.height; radius: 2; color: win.accentEdge }
                }
                handle: Rectangle {
                    x: padSlider.leftPadding + padSlider.visualPosition * (padSlider.availableWidth - width)
                    y: padSlider.topPadding + padSlider.availableHeight / 2 - height / 2
                    width: 18; height: 18; radius: 9
                    color: "white"; border.color: win.accentEdge; border.width: 2
                }
            }
            Text { textFormat: Text.PlainText; text: Math.round(win.padding) + " px"; color: win.ink; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
        }

        // trim: two times
        Row {
            visible: edit.tool === "trim"
            anchors { left: parent.left; top: parent.top; margins: 16 }
            spacing: 14
            Text { textFormat: Text.PlainText; text: "From"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField {
                id: startField
                text: "0:00"
                Accessible.name: "Start time"
            }
            Text { textFormat: Text.PlainText; text: "to"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField {
                id: endField
                text: win.total > 0 ? win.clock(win.total) : ""
                Accessible.name: "End time"
            }
        }
        Text { textFormat: Text.PlainText;
            visible: edit.tool === "trim"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 62 }
            text: win.trimOk ? "Keeps " + win.clock(Math.min(win.tEnd, win.total > 0 ? win.total : win.tEnd) - win.tStart)
                             : "Use seconds, mm:ss or hh:mm:ss. The end has to come after the start."
            color: win.trimOk ? win.muted : "#D6643A"
            font.pixelSize: 13
            wrapMode: Text.WordWrap
        }

        // annotate: the tool, the colour and the size
        Flow {
            id: annotTools
            visible: edit.tool === "annotate"
            // The tools on the left; Undo and Clear on the right, apart from them.
            anchors { left: parent.left; right: annotEdit.left; top: parent.top; margins: 16 }
            spacing: 8
            Repeater {
                model: [["pen", "Pen", "draw-freehand"], ["arrow", "Arrow", "draw-arrow"], ["box", "Box", "arka-box"],
                        ["highlight", "Highlight", "draw-highlight"], ["text", "Text", "draw-text"]]
                delegate: Btn {
                    required property var modelData
                    text: modelData[1]
                    iconName: modelData[2]
                    selected: win.pen === modelData[0]
                    onClicked: win.pen = modelData[0]
                }
            }
        }
        Row {
            id: annotSwatches
            visible: edit.tool === "annotate"
            anchors { left: parent.left; top: annotTools.bottom; leftMargin: 16; topMargin: 10 }
            spacing: 8
            Repeater {
                model: win.inks
                delegate: Rectangle {
                    required property string modelData
                    width: 30; height: 30; radius: 15
                    color: modelData
                    border.width: 2
                    border.color: win.dark ? "#8A93A0" : win.edge   // 3:1+, so white and black swatches show on either look
                    Rectangle {   // the chosen colour: a ring with a gap around the swatch
                        visible: win.inkColor === modelData
                        anchors.centerIn: parent; width: parent.width + 8; height: width; radius: width / 2
                        color: "transparent"; border.color: win.accentEdge; border.width: 2
                    }
                    Text { textFormat: Text.PlainText; anchors.centerIn: parent; visible: win.inkColor === modelData; text: "✓"
                           font.pixelSize: 14; font.bold: true
                           color: (parseInt(modelData.substr(1, 2), 16) + parseInt(modelData.substr(3, 2), 16) + parseInt(modelData.substr(5, 2), 16)) > 382 ? "#14181F" : "#FFFFFF" }
                    MouseArea { id: inkMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                                onClicked: win.inkColor = modelData }
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: win.inkNames[win.inks.indexOf(modelData)] || ("Colour " + modelData)
                    Tip { visible: inkMouse.containsMouse; text: win.inkNames[win.inks.indexOf(modelData)] || modelData }
                    Accessible.checked: win.inkColor === modelData
                    Accessible.onPressAction: win.inkColor = modelData
                }
            }
            Item { width: 12; height: 1 }
            Text { textFormat: Text.PlainText; text: "Size"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            Repeater {
                model: ["Small", "Medium", "Large"]
                delegate: Btn {
                    required property string modelData
                    required property int index
                    text: modelData.charAt(0)
                    dot: [4, 7, 11][index]          // the size itself, not a letter
                    implicitWidth: 40
                    Tip { visible: parent.hovered; text: modelData + " lines and letters" }
                    selected: win.sizeIx === index
                    onClicked: win.sizeIx = index
                    Accessible.name: modelData + " lines and letters"
                }
            }
        }
        Row {
            id: annotEdit
            visible: edit.tool === "annotate"
            anchors { right: parent.right; top: parent.top; margins: 16 }
            spacing: 8
            Btn { text: "Undo"; enabledState: win.shapes.length > 0; onClicked: win.undoShape() }
            // Clears every mark: a quiet text button, so it is not taken for the main action.
            Btn { text: "Clear all"; flat: true; enabledState: win.shapes.length > 0; onClicked: win.shapes = [] }
        }

        // video redact: the whole video, or only a while
        Row {
            id: redactWhile
            visible: edit.tool === "redact" && edit.isVideo
            anchors { left: parent.left; top: redactAdvice.bottom; leftMargin: 16; topMargin: 12 }
            spacing: 10
            Text { textFormat: Text.PlainText; text: "Only from"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField { id: rFromField; placeholderText: "start"; Accessible.name: "Cover from" }
            Text { textFormat: Text.PlainText; text: "to"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField { id: rToField; placeholderText: "end"; Accessible.name: "Cover until" }
            Text { textFormat: Text.PlainText;
                anchors.verticalCenter: parent.verticalCenter
                text: win.rWhole ? "Empty: covered for the whole video" : win.rOk ? "" : "The end has to come after the start"
                color: win.rOk ? win.muted : "#D6643A"
                font.pixelSize: 13
            }
        }

        // split: one time
        Row {
            visible: edit.tool === "split"
            anchors { left: parent.left; top: parent.top; margins: 16 }
            spacing: 14
            Text { textFormat: Text.PlainText; text: "Cut at"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField { id: splitField; text: win.total > 0 ? win.clock(win.total / 2) : ""; Accessible.name: "Cut at" }
        }
        Text { textFormat: Text.PlainText;
            visible: edit.tool === "split"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 62 }
            text: win.splitOk ? (win.total > 0 ? "Part 1 is " + win.clock(win.tSplit) + " long, part 2 is " + win.clock(win.total - win.tSplit)
                                               : "Two files: before and after " + win.clock(win.tSplit))
                              : "Use seconds, mm:ss or hh:mm:ss, inside the file."
            color: win.splitOk ? win.muted : "#D6643A"
            font.pixelSize: 13
        }

        // bleep: add parts, see them, remove them
        Row {
            visible: edit.tool === "bleep"
            anchors { left: parent.left; top: parent.top; margins: 16 }
            spacing: 10
            Text { textFormat: Text.PlainText; text: "From"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField { id: fromField; Accessible.name: "Bleep from"; onAccepted: toField.forceActiveFocus() }
            Text { textFormat: Text.PlainText; text: "to"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField { id: toField; Accessible.name: "Bleep until"; onAccepted: win.addPart() }
            Btn { text: "Add part"; enabledState: win.partOk; onClicked: win.addPart() }
            Text { textFormat: Text.PlainText;   // why "Add part" is not ready, said instead of a silently dim button
                anchors.verticalCenter: parent.verticalCenter
                visible: !win.partOk
                readonly property bool empty: fromField.text.length === 0 || toField.text.length === 0
                text: empty ? "Type a start and an end" : "The end has to come after the start"
                color: empty ? win.muted : "#D6643A"; font.pixelSize: 13
            }
        }
        Flow {
            visible: edit.tool === "bleep"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 60 }
            spacing: 8
            Text { textFormat: Text.PlainText;
                width: parent.width
                wrapMode: Text.WordWrap
                text: (parts.count === 0 ? "No parts yet: drag on the timeline, or type a start and an end and press Enter. " : "")
                      + "In the copy, the sound in each part is removed and a tone plays instead; the rest is kept."
                      + (edit.isVideo ? " The picture is not changed." : "")
                color: win.muted; font.pixelSize: 13
            }
            Repeater {
                model: parts
                delegate: Rectangle {
                    required property real a
                    required property real b
                    required property int index
                    height: 32; radius: 16
                    width: chip.implicitWidth + 40
                    color: win.bg; border.color: win.edge
                    Text { textFormat: Text.PlainText; id: chip; x: 12; anchors.verticalCenter: parent.verticalCenter; text: win.clock(a) + " – " + win.clock(b); color: win.ink; font.pixelSize: 12 }
                    Text { textFormat: Text.PlainText;
                        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        text: "×"; color: win.muted; font.pixelSize: 15
                        MouseArea { anchors.centerIn: parent; width: 28; height: 28; cursorShape: Qt.PointingHandCursor
                                    onClicked: parts.remove(index) }
                        Accessible.role: Accessible.Button
                        Accessible.name: "Remove " + win.clock(a) + " to " + win.clock(b)
                        Accessible.onPressAction: parts.remove(index)
                    }
                }
            }
        }

        // footer: a divider, a hint while the main button is not ready, then the buttons
        Text { textFormat: Text.PlainText
            anchors { left: parent.left; leftMargin: 16; bottom: parent.bottom; bottomMargin: 24 }
            visible: !win.canApply
            text: ({ crop: "Nothing to save yet: the box is still the whole picture", redact: "Nothing to save yet: no boxes",
                     annotate: "Nothing to save yet: nothing drawn", trim: "Nothing to save yet: check the times",
                     split: "Nothing to save yet: the cut has to be inside the file", bleep: "Nothing to save yet: no parts" })[edit.tool] ?? ""
            color: win.muted; font.pixelSize: 13
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom; bottomMargin: controls.footer }
                    height: 1; color: win.line }
        Row {
            anchors { right: parent.right; bottom: parent.bottom; rightMargin: 16; bottomMargin: 14 }
            spacing: 10
            Btn { text: "Cancel"; onClicked: Qt.quit() }
            Btn {
                text: ({ crop: "Save cropped copy", redact: "Save blacked-out copy", annotate: "Save annotated copy",
                         background: "Save with background", trim: "Save trimmed copy", split: "Save both parts",
                         bleep: "Save bleeped copy" })[edit.tool]
                primary: true
                enabledState: win.canApply
                onClicked: win.apply()
            }
        }
    }
}
