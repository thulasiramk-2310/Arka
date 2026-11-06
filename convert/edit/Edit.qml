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
        // "Hide private text" on the wheel: start by suggesting boxes; the person still checks and saves.
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
    readonly property color accent: "#4DB491"
    readonly property color accentInk: "#06281B"

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
                const b = f.box
                let dup = false
                for (let i = 0; i < boxes.count; i++) {
                    const o = boxes.get(i)
                    if (Math.abs(o.bx - b[0]) < 2 && Math.abs(o.by - b[1]) < 2 && Math.abs(o.bw - b[2]) < 2) dup = true
                }
                if (dup) continue
                boxes.append({ bx: b[0], by: b[1], bw: Math.min(b[2], imgSize.width - b[0]), bh: Math.min(b[3], imgSize.height - b[1]) })
                added++
                if (kinds.indexOf(names[f.kind] || f.kind) < 0) kinds.push(names[f.kind] || f.kind)
            }
            win.findNote = added ? "Found " + added + ": " + kinds.join(", ") : "None found"
            win.findAdvice = "Text recognition can miss things or box too much: check the picture before saving. "
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
             : (btnMouse.containsMouse && enabledState ? win.line : win.panel)
        border.width: activeFocus ? 2 : 1
        border.color: activeFocus ? win.accent : (primary && enabledState ? win.ink : selected ? win.accent : win.line)
        Keys.onSpacePressed: if (enabledState) clicked()
        Keys.onReturnPressed: if (enabledState) clicked()
        Image {
            visible: btn.iconName.length > 0
            anchors { verticalCenter: parent.verticalCenter; right: label.left; rightMargin: 8 }
            width: 18; height: 18; sourceSize: Qt.size(36, 36)
            source: btn.iconName.length ? "image://icon/" + btn.iconName : ""
        }
        Text { textFormat: Text.PlainText;
            id: label
            anchors.centerIn: parent
            anchors.horizontalCenterOffset: btn.iconName.length ? 12 : 0
            text: btn.text
            color: btn.primary ? (btn.enabledState ? win.bg : win.muted) : btn.selected ? win.accentInk : win.ink
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

    component TimeField: TextField {
        id: tf
        width: 110
        color: win.ink
        selectByMouse: true
        placeholderText: "0:00"
        placeholderTextColor: win.muted
        background: Rectangle { radius: 6; color: win.bg; border.width: tf.activeFocus ? 2 : 1
                                border.color: tf.activeFocus ? win.accent : win.line }
    }


    // ---- inspector: what this file is, on the right; the button at the top right hides it
    readonly property var facts: edit.details()
    // What the copy will be, as far as it is known from the choices (never an estimate of file size).
    readonly property string resultText: {
        if (edit.tool === "crop") return Math.round(cropW) + " × " + Math.round(cropH) + " px"
        if (edit.tool === "trim") return trimOk ? clock(Math.min(tEnd, total > 0 ? total : tEnd) - tStart) + " long" : ""
        if (edit.tool === "split") return splitOk && total > 0 ? clock(tSplit) + " + " + clock(total - tSplit) : ""
        if (edit.tool === "bleep") return parts.count ? parts.count + (parts.count === 1 ? " part bleeped" : " parts bleeped") : ""
        if (edit.tool === "redact") return boxes.count ? boxes.count + (boxes.count === 1 ? " box" : " boxes") : ""
        if (edit.tool === "background") return Math.round(imgSize.width + 2 * padding) + " × " + Math.round(imgSize.height + 2 * padding) + " px"
        return ""
    }
    Rectangle {
        id: inspectorToggle
        z: 3
        anchors { right: parent.right; top: parent.top; margins: 10 }
        width: toggleLabel.implicitWidth + 50; height: 34; radius: 8
        // Flat, like a toolbar button: the accent only marks the icon while the panel is open.
        color: toggleMouse.containsMouse ? win.line : "transparent"
        border.color: inspectorToggle.activeFocus ? win.accent : "transparent"
        border.width: 2
        activeFocusOnTab: true
        Keys.onSpacePressed: edit.inspectorOpen = !edit.inspectorOpen
        // a panel icon: a frame with its right part filled
        Rectangle {
            anchors { left: parent.left; leftMargin: 11; verticalCenter: parent.verticalCenter }
            width: 16; height: 13; radius: 2; color: "transparent"
            border.width: 1.5; border.color: edit.inspectorOpen ? win.accent : win.muted
            Rectangle { anchors { right: parent.right; top: parent.top; bottom: parent.bottom; margins: 2 } width: 5; radius: 1
                        color: edit.inspectorOpen ? win.accent : win.muted }
        }
        property bool justClicked: false
        Text { id: toggleLabel; textFormat: Text.PlainText
               anchors { left: parent.left; leftMargin: 34; verticalCenter: parent.verticalCenter }
               text: "Details"; color: edit.inspectorOpen ? win.accent : win.muted; font.pixelSize: 12; font.weight: Font.DemiBold }
        MouseArea { id: toggleMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                    onClicked: { inspectorToggle.justClicked = true; edit.inspectorOpen = !edit.inspectorOpen }
                    onExited: inspectorToggle.justClicked = false }
        ToolTip.visible: toggleMouse.containsMouse && !justClicked
        ToolTip.delay: 500
        ToolTip.text: (edit.inspectorOpen ? "Hide" : "Show") + " details (Ctrl+I)"
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
        Flickable {
            id: inspectorScroll
            anchors { fill: parent; topMargin: 52 }
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
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
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
                Rectangle {   // the file itself, small
                    width: parent.width; height: 124; radius: 10
                    color: win.bg; border.color: win.line
                    // The picture, a frame of the video, or the sound's waveform.
                    Image {
                        anchors { fill: parent; margins: 10 }
                        fillMode: win.timeTool && !edit.hasFrame ? Image.Stretch : Image.PreserveAspectFit
                        autoTransform: true; smooth: true; asynchronous: true
                        sourceSize.width: 440
                        source: win.timeTool ? (edit.hasFrame ? edit.pictureUrl : edit.waveUrl) : edit.pictureUrl
                    }
                }
                Column {
                    width: parent.width; spacing: 3
                    Text { textFormat: Text.PlainText; width: parent.width; text: edit.fileName; color: win.ink; font.pixelSize: 15; font.weight: Font.DemiBold
                           horizontalAlignment: Text.AlignHCenter; elide: Text.ElideMiddle }
                    Text { textFormat: Text.PlainText; width: parent.width; text: win.facts.kind + " · " + win.facts.format; color: win.muted; font.pixelSize: 12
                           horizontalAlignment: Text.AlignHCenter }
                }
                Btn { width: parent.width; text: "Open"; primary: true; onClicked: edit.openFile() }
                Btn { width: parent.width; text: "Show in folder"; onClicked: edit.showInFolder() }
                // What the file carries besides what you see: found by mat2, removed in a copy on request.
                Text { textFormat: Text.PlainText; text: "Hidden details"; color: win.muted; font.pixelSize: 12; font.weight: Font.DemiBold }
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
                                     ? "Nothing personal found by mat2; only how the file is stored. "
                                     : "mat2 found none in this file. ")
                                    + "It can't read every kind of detail in every format."
                            color: win.muted; font.pixelSize: 12
                        }
                        Repeater {
                            model: hiddenCol.groups
                            delegate: Column {
                                required property var modelData
                                width: hiddenCol.width; spacing: 2
                                Text { textFormat: Text.PlainText; text: modelData[0]; color: modelData[0] === "Location" ? (win.dark ? "#E07A50" : "#B4481F") : win.ink
                                       font.pixelSize: 12; font.weight: Font.DemiBold }
                                Text { textFormat: Text.PlainText;
                                    width: parent.width; elide: Text.ElideRight; color: win.muted; font.pixelSize: 12
                                    // The most telling value first, e.g. the whole position for a location.
                                    text: {
                                        const rows = modelData[1]
                                        const pos = rows.find(r => /position/i.test(r[0]))
                                        const pick = pos ? [pos] : rows.slice(0, 2)
                                        return pick.map(r => r[1]).join(" · ") + (rows.length > pick.length ? "  +" + (rows.length - pick.length) + " more" : "")
                                    }
                                }
                            }
                        }
                        Btn {
                            id: removeHiddenBtn
                            visible: hiddenCol.groups.length > 0
                            width: parent.width
                            property bool started: false
                            text: started ? "Saving a copy without them…" : "Remove hidden details"
                            iconName: "document-cleanup"
                            enabledState: !started
                            onClicked: started = edit.removeHidden()
                        }
                    }
                }
                Text { textFormat: Text.PlainText; text: "Details"; color: win.muted; font.pixelSize: 12; font.weight: Font.DemiBold }
                Rectangle {
                    width: parent.width; radius: 10; color: win.bg; border.color: win.line
                    height: rows.height + 8
                    Column {
                        id: rows
                        y: 4; width: parent.width
                        Repeater {
                            model: {
                                const rows = [["Size", win.facts.size]]
                                if (win.isImage) rows.push(["Dimensions", win.imgSize.width + " × " + win.imgSize.height + " px"])
                                if (win.timeTool || edit.isVideo) rows.push(["Length", win.total > 0 ? win.clock(win.total) : "unknown"])
                                rows.push(["Format", win.facts.format], ["Changed", win.facts.changed], ["Folder", win.facts.folder])
                                if (win.resultText.length) rows.push(["Result", win.resultText])
                                return rows
                            }
                            delegate: Item {
                                required property var modelData
                                required property int index
                                width: rows.width; height: 34
                                Rectangle { visible: index > 0; anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 12; rightMargin: 12 }
                                            height: 1; color: win.line }
                                Text { textFormat: Text.PlainText; anchors { left: parent.left; leftMargin: 12; verticalCenter: parent.verticalCenter }
                                       text: modelData[0]; color: win.muted; font.pixelSize: 12 }
                                Text { textFormat: Text.PlainText; id: rowValue; anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                                       width: parent.width - 110; horizontalAlignment: Text.AlignRight
                                       elide: modelData[0] === "Folder" ? Text.ElideLeft : Text.ElideMiddle
                                       text: modelData[1]; color: modelData[0] === "Result" ? win.accent : win.ink
                                       font.pixelSize: 12; font.weight: modelData[0] === "Result" ? Font.DemiBold : Font.Normal }
                                MouseArea { id: rowHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                                ToolTip.visible: rowHover.containsMouse && rowValue.truncated
                                ToolTip.text: win.plain(modelData[1])   // tooltips read HTML: show file text literally
                            }
                        }
                    }
                }
                Text { textFormat: Text.PlainText;
                    width: parent.width; wrapMode: Text.WordWrap
                    // True of every edit here: arka-convert runs its tools in a sandbox with no network.
                    text: "Edits are saved as a new file next to this one; this file is not changed. The work is done on this computer, with no network access."
                    color: win.muted; font.pixelSize: 12
                }
            }
        }
    }

    // ---- header, as ArkaOS's own apps have it: a serif title, then what to do in plain words
    Column {
        id: heading
        anchors { left: parent.left; right: inspector.left; top: parent.top; leftMargin: 24; rightMargin: 64; topMargin: 18 }
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
            text: ({ crop: edit.isVideo ? "Drag the corners to choose what to keep. The whole video is cropped the same way."
                                        : "Drag the corners to choose what to keep.",
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
        anchors { left: parent.left; right: inspector.left; top: heading.bottom; bottom: controls.top; topMargin: 0 }

        Image {
            id: img
            anchors.fill: parent
            anchors.margins: 16
            fillMode: Image.PreserveAspectFit
            autoTransform: true
            smooth: true
            source: win.picTool ? edit.pictureUrl : ""
        }
        readonly property real sc: win.imgSize.width > 0 ? img.paintedWidth / win.imgSize.width : 1
        readonly property real ox: img.x + (img.width - img.paintedWidth) / 2
        readonly property real oy: img.y + (img.height - img.paintedHeight) / 2
        function px(v) { return (v - ox) / sc }
        function py(v) { return (v - oy) / sc }

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
                color: "transparent"; border.color: win.accent; border.width: 2
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
                        color: Qt.rgba(1, 1, 1, 0.45)
                    }
                }
            }
            Repeater {
                model: 4
                delegate: Rectangle {
                    required property int index
                    width: 18; height: 18; radius: 4
                    color: win.accent; border.color: win.accentInk; border.width: 1
                    x: cropLayer.bx + (index % 2 === 0 ? 0 : cropLayer.bw) - width / 2
                    y: cropLayer.by + (index < 2 ? 0 : cropLayer.bh) - height / 2
                }
            }
            // One mouse area does the work, so a drag is not lost when the box moves under it.
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                property int mode: -1       // 0-3 a corner, 4 the whole box, -1 nothing
                property real gx: 0
                property real gy: 0
                function hit(x, y) {
                    for (let i = 0; i < 4; i++) {
                        const cx = cropLayer.bx + (i % 2 === 0 ? 0 : cropLayer.bw)
                        const cy = cropLayer.by + (i < 2 ? 0 : cropLayer.bh)
                        if (Math.abs(x - cx) <= 18 && Math.abs(y - cy) <= 18) return i
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
                                      : h === 4 ? Qt.OpenHandCursor : Qt.ArrowCursor
                        return
                    }
                    if (mode >= 0 && mode < 4) win.resizeFrom(mode, stage.px(p.x), stage.py(p.y))
                    else if (mode === 4) win.setBox(stage.px(p.x) - gx, stage.py(p.y) - gy, win.cropW, win.cropH)
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
                    border.color: win.accent; border.width: 1
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
                text: edit.fileName
                color: win.ink
                font.pixelSize: 16
                font.weight: Font.DemiBold
                elide: Text.ElideMiddle
                width: parent.width
            }
            // A video shows here (a frame until it plays); sound has only the timeline.
            Item {
                id: videoBox
                width: parent.width
                height: edit.isVideo ? Math.max(100, Math.min(280, (trimStage.height - 140) * 0.55)) : 0
                visible: edit.isVideo
                Image { anchors.fill: parent; fillMode: Image.PreserveAspectFit; source: edit.hasFrame ? edit.pictureUrl : ""
                        visible: !(player.item && player.item.playing) }
                Loader { id: player; anchors.fill: parent; source: "Player.qml"
                         onLoaded: item.source = edit.fileUrl }
            }
            Row {
                spacing: 12
                Btn {
                    id: playBtn
                    visible: player.status === Loader.Ready && win.total > 0
                    iconName: player.item && player.item.playing ? "media-playback-pause" : "media-playback-start"
                    text: player.item && player.item.playing ? "Stop" : ({ trim: "Play the part kept", split: "Play around the cut",
                                                                           bleep: "Play" })[edit.tool] ?? "Play"
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
                width: parent.width; height: Math.max(96, Math.min(300, trimStage.height - videoBox.height - 120)); radius: 10
                color: win.dark ? "#16191E" : "#E7EAEE"
                border.width: activeFocus ? 2 : 1
                border.color: activeFocus ? win.accent : win.line
                clip: true
                // The keyboard way: Left/Right move the cut, or the start of a trim; Shift moves the end of a
                // trim; Ctrl takes whole seconds. Bleep parts are typed in the fields below.
                activeFocusOnTab: live && edit.tool !== "bleep"
                Accessible.role: Accessible.Slider
                Accessible.name: edit.tool === "trim" ? "Part to keep" : edit.tool === "split" ? "Where to cut" : "Timeline"
                Accessible.description: edit.tool === "trim" ? "From " + win.clock(win.tStart) + " to " + win.clock(win.tEnd)
                                      : edit.tool === "split" ? "Cut at " + win.clock(win.tSplit) : "Drag across a part to bleep it out"
                function nudge(ev, dir) {
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
                    anchors { fill: parent; margins: 8 }
                    source: edit.waveUrl
                    fillMode: Image.Stretch
                    smooth: true
                    opacity: 0.9
                }
                Text { textFormat: Text.PlainText;
                    visible: edit.waveUrl.toString() === ""
                    anchors.centerIn: parent
                    text: bar.live ? "No sound to draw" : "The length could not be read, so type the times below"
                    color: win.muted; font.pixelSize: 12
                }
                // trim: dim what is cut away, mark the two ends
                Rectangle { visible: edit.tool === "trim" && win.trimOk && bar.live; x: 0; width: bar.xAt(win.tStart); height: parent.height
                            color: Qt.rgba(0, 0, 0, win.dark ? 0.55 : 0.30) }
                Rectangle { visible: edit.tool === "trim" && win.trimOk && bar.live; x: bar.xAt(Math.min(win.tEnd, bar.span))
                            width: parent.width - x; height: parent.height; color: Qt.rgba(0, 0, 0, win.dark ? 0.55 : 0.30) }
                Repeater {
                    model: edit.tool === "trim" && win.trimOk && bar.live ? [win.tStart, Math.min(win.tEnd, bar.span)] : []
                    delegate: Rectangle {
                        required property real modelData
                        x: bar.xAt(modelData) - 2; width: 4; height: bar.height; radius: 2; color: win.accent
                        Rectangle { anchors.centerIn: parent; width: 12; height: 28; radius: 4; color: win.accent }
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
                }
                // bleep: the parts, and the one being drawn
                Repeater {
                    model: edit.tool === "bleep" && bar.live ? parts : null
                    delegate: Rectangle {
                        required property real a
                        required property real b
                        x: bar.xAt(a); width: Math.max(3, bar.xAt(b) - bar.xAt(a)); height: bar.height
                        color: Qt.rgba(229 / 255, 72 / 255, 77 / 255, 0.35); border.color: "#E5484D"
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
            Item {   // the ends of the timeline
                width: parent.width; height: 16
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
                             redact: edit.isVideo ? 156 : 104 })[edit.tool] ?? 104)
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
                iconName: "edit-find"
                enabledState: !busy
                onClicked: { busy = true; win.findNote = ""; edit.findPrivate() }
            }
            Btn { text: "Undo last"; enabledState: boxes.count > 0; onClicked: boxes.remove(boxes.count - 1) }
            Btn { text: "Clear all"; enabledState: boxes.count > 0; onClicked: boxes.clear() }
            Text { textFormat: Text.PlainText;
                anchors.verticalCenter: parent.verticalCenter
                text: win.findNote.length ? win.findNote
                      : boxes.count === 0 ? "Nothing covered yet" : boxes.count + (boxes.count === 1 ? " part" : " parts") + " will be covered"
                color: win.muted
                font.pixelSize: 13
            }
        }
        Text { textFormat: Text.PlainText;
            visible: edit.tool === "redact"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 58 }
            text: win.findAdvice + (edit.isVideo
                  ? "The boxes are painted into the copy's frames, so the picture under them is gone from it; the title and other details are removed too.\nThe sound is kept as it is: use Bleep for words. The original is not changed."
                  : (win.findNote.length ? "" : "Find private text looks for email addresses, phone and card numbers, UPI IDs and IBANs; not names or addresses. ")
                    + "The covered pixels are replaced in the copy, so what was under them is not in it. The original is not changed.")
            color: win.muted
            font.pixelSize: 12
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
                    border.color: win.swatch === index ? win.accent : win.line
                    gradient: Gradient {
                        GradientStop { position: 0; color: c[0] }
                        GradientStop { position: 1; color: c[c.length - 1] }
                    }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: win.swatch = index }
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: win.swatchNames[index] || ("Colour " + modelData)
                    Accessible.checked: win.swatch === index
                    Accessible.onPressAction: win.swatch = index
                }
            }
            Btn { text: "Custom…"; onClicked: colourPicker.open(); Accessible.name: "Choose your own colour" }
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
                    color: win.line
                    Rectangle { width: padSlider.visualPosition * parent.width; height: parent.height; radius: 2; color: win.accent }
                }
                handle: Rectangle {
                    x: padSlider.leftPadding + padSlider.visualPosition * (padSlider.availableWidth - width)
                    y: padSlider.topPadding + padSlider.availableHeight / 2 - height / 2
                    width: 18; height: 18; radius: 9
                    color: "white"; border.color: win.accent; border.width: 2
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
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 16 }
            spacing: 6
            Repeater {
                model: [["pen", "Pen", "draw-freehand"], ["arrow", "Arrow", "draw-arrow"], ["box", "Box", "draw-rectangle"],
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
                    width: 26; height: 26; radius: 13
                    color: modelData
                    border.width: win.inkColor === modelData ? 3 : 1
                    border.color: win.inkColor === modelData ? win.accent : Qt.rgba(win.ink.r, win.ink.g, win.ink.b, 0.4)
                    Text { textFormat: Text.PlainText; anchors.centerIn: parent; visible: win.inkColor === modelData; text: "✓"
                           font.pixelSize: 14; font.bold: true
                           color: (parseInt(modelData.substr(1, 2), 16) + parseInt(modelData.substr(3, 2), 16) + parseInt(modelData.substr(5, 2), 16)) > 382 ? "#14181F" : "#FFFFFF" }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: win.inkColor = modelData }
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: "Colour " + modelData
                    Accessible.checked: win.inkColor === modelData
                    Accessible.onPressAction: win.inkColor = modelData
                }
            }
            Item { width: 12; height: 1 }
            Repeater {
                model: ["Small", "Medium", "Large"]
                delegate: Btn {
                    required property string modelData
                    required property int index
                    text: modelData.charAt(0)
                    implicitWidth: 40
                    ToolTip.visible: hovered
                    ToolTip.text: modelData + " lines and letters"
                    property bool hovered: false
                    HoverHandler { onHoveredChanged: parent.hovered = hovered }
                    selected: win.sizeIx === index
                    onClicked: win.sizeIx = index
                    Accessible.name: modelData + " lines and letters"
                }
            }
            Item { width: 12; height: 1 }
            Btn { text: "Undo"; enabledState: win.shapes.length > 0; onClicked: win.undoShape() }
            Btn { text: "Clear"; enabledState: win.shapes.length > 0; onClicked: win.shapes = [] }
        }

        // video redact: the whole video, or only a while
        Row {
            visible: edit.tool === "redact" && edit.isVideo
            anchors { left: parent.left; top: parent.top; leftMargin: 16; topMargin: 104 }
            spacing: 10
            Text { textFormat: Text.PlainText; text: "Only from"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField { id: rFromField; placeholderText: "start"; Accessible.name: "Cover from" }
            Text { textFormat: Text.PlainText; text: "to"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TimeField { id: rToField; placeholderText: "end"; Accessible.name: "Cover until" }
            Text { textFormat: Text.PlainText;
                anchors.verticalCenter: parent.verticalCenter
                text: win.rWhole ? "Empty: covered for the whole video" : win.rOk ? "" : "The end has to come after the start"
                color: win.rOk ? win.muted : "#D6643A"
                font.pixelSize: 12
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
                color: empty ? win.muted : "#D6643A"; font.pixelSize: 12
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
                color: win.muted; font.pixelSize: 12
            }
            Repeater {
                model: parts
                delegate: Rectangle {
                    required property real a
                    required property real b
                    required property int index
                    height: 28; radius: 14
                    width: chip.implicitWidth + 40
                    color: win.bg; border.color: win.line
                    Text { textFormat: Text.PlainText; id: chip; x: 12; anchors.verticalCenter: parent.verticalCenter; text: win.clock(a) + " – " + win.clock(b); color: win.ink; font.pixelSize: 12 }
                    Text { textFormat: Text.PlainText;
                        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        text: "×"; color: win.muted; font.pixelSize: 15
                        MouseArea { anchors.fill: parent; anchors.margins: -6; cursorShape: Qt.PointingHandCursor; onClicked: parts.remove(index) }
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
            color: win.muted; font.pixelSize: 12
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom; bottomMargin: controls.footer }
                    height: 1; color: win.line }
        Row {
            anchors { right: parent.right; bottom: parent.bottom; rightMargin: 16; bottomMargin: 14 }
            spacing: 10
            Btn { text: "Cancel"; onClicked: Qt.quit() }
            Btn {
                text: ({ crop: "Save cropped copy", redact: "Save blacked-out copy", annotate: "Save copy",
                         background: "Save with background", trim: "Save trimmed copy", split: "Save both parts",
                         bleep: "Save bleeped copy" })[edit.tool]
                primary: true
                enabledState: win.canApply
                onClicked: win.apply()
            }
        }
    }
}
