// arka-edit: crop, black out parts, add a background, trim. See main.cpp.
import QtQuick
import QtQuick.Window
import QtQuick.Controls

Window {
    id: win
    width: 780
    height: 660
    minimumWidth: 640
    minimumHeight: 520
    visible: true
    title: ({ crop: "Crop picture", redact: "Black out parts", background: "Add a background", trim: "Trim" })[edit.tool]
           + " — " + edit.fileName
    color: bg

    readonly property bool dark: edit.dark()
    readonly property color bg: dark ? "#1B1E23" : "#F1F3F5"
    readonly property color panel: dark ? "#23272E" : "#FFFFFF"
    readonly property color ink: dark ? "#ECEFF3" : "#14181F"
    readonly property color muted: dark ? "#A3ABB6" : "#5B6472"
    readonly property color line: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0.07, 0.1, 0.15, 0.12)
    readonly property color accent: "#16C784"
    readonly property color accentInk: "#06281B"

    readonly property bool isImage: edit.tool !== "trim"
    readonly property bool picTool: edit.tool === "crop" || edit.tool === "redact"
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
    property var drawing: null   // {x, y, w, h} in picture pixels while dragging

    // ---- background state
    readonly property var swatches: ["#FFFFFF", "#F2EFE9", "#1E2128", "#16C784", "#2F6FED",
                                     "#FFD9B8-#F58A7A", "#BFD9FF-#7A8CF5", "#CFF5E3-#3FC6A0", "#2B3140-#0E1117"]
    property int swatch: 5
    property real padding: 60

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

    readonly property bool canApply: {
        if (edit.tool === "crop") return cropW > 0 && (cropW < imgSize.width || cropH < imgSize.height)
        if (edit.tool === "redact") return boxes.count > 0
        if (edit.tool === "background") return true
        return trimOk
    }
    function apply() {
        if (!canApply) return
        if (edit.tool === "crop") {
            edit.apply([cropX, cropY, cropW, cropH].map(v => String(Math.round(v))))
        } else if (edit.tool === "redact") {
            const a = []
            for (let i = 0; i < boxes.count; i++) {
                const b = boxes.get(i)
                a.push(String(Math.round(b.bx)), String(Math.round(b.by)), String(Math.round(b.bw)), String(Math.round(b.bh)))
            }
            edit.apply(a)
        } else if (edit.tool === "background") {
            edit.apply([swatches[swatch], String(Math.round(padding))])
        } else {
            edit.apply([startField.text.trim(), endField.text.trim()])
        }
    }

    Shortcut { sequence: "Escape"; onActivated: Qt.quit() }
    Shortcut { sequences: ["Return", "Enter"]; enabled: !startField.activeFocus && !endField.activeFocus; onActivated: win.apply() }

    component Btn: Rectangle {
        id: btn
        property string text
        property bool primary: false
        property bool enabledState: true
        signal clicked
        implicitWidth: label.implicitWidth + 36
        implicitHeight: 34
        radius: 8
        opacity: enabledState ? 1 : 0.4
        color: primary ? win.accent : win.panel
        border.color: primary ? Qt.darker(win.accent, 1.12) : win.line
        // a soft highlight along the top, nothing more
        Rectangle {
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 1 }
            height: parent.height / 2
            radius: 7
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(1, 1, 1, btn.primary ? 0.28 : (win.dark ? 0.07 : 0.55)) }
                GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0) }
            }
        }
        Text {
            id: label
            anchors.centerIn: parent
            text: btn.text
            color: btn.primary ? win.accentInk : win.ink
            font.pixelSize: 13
            font.weight: Font.DemiBold
        }
        MouseArea {
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

    // ---- header
    Text {
        id: heading
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 16 }
        text: ({ crop: "Drag the corners to choose what to keep",
                 redact: "Drag over anything you want covered with black",
                 background: "Pick a colour and how much room to leave around the picture",
                 trim: "Choose the part to keep" })[edit.tool]
        color: win.muted
        font.pixelSize: 13
        elide: Text.ElideRight
    }

    // ---- picture stage (crop and redact)
    Item {
        id: stage
        visible: win.picTool
        anchors { left: parent.left; right: parent.right; top: heading.bottom; bottom: controls.top; margins: 16 }

        Image {
            id: img
            anchors.fill: parent
            fillMode: Image.PreserveAspectFit
            autoTransform: true
            smooth: true
            source: win.picTool ? edit.fileUrl : ""
        }
        readonly property real sc: win.imgSize.width > 0 ? img.paintedWidth / win.imgSize.width : 1
        readonly property real ox: (width - img.paintedWidth) / 2
        readonly property real oy: (height - img.paintedHeight) / 2
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
            }
            Repeater {
                model: 4
                delegate: Rectangle {
                    required property int index
                    width: 14; height: 14; radius: 3
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
                        if (Math.abs(x - cx) <= 14 && Math.abs(y - cy) <= 14) return i
                    }
                    if (x >= cropLayer.bx && x <= cropLayer.bx + cropLayer.bw && y >= cropLayer.by && y <= cropLayer.by + cropLayer.bh) return 4
                    return -1
                }
                onPressed: (m) => {
                    mode = hit(m.x, m.y)
                    if (mode === 4) { gx = stage.px(m.x) - win.cropX; gy = stage.py(m.y) - win.cropY }
                }
                onReleased: mode = -1
                onPositionChanged: (m) => {
                    if (!pressed) {
                        const h = hit(m.x, m.y)
                        cursorShape = h === 0 || h === 3 ? Qt.SizeFDiagCursor : h === 1 || h === 2 ? Qt.SizeBDiagCursor
                                      : h === 4 ? Qt.OpenHandCursor : Qt.ArrowCursor
                        return
                    }
                    if (mode >= 0 && mode < 4) win.resizeFrom(mode, stage.px(m.x), stage.py(m.y))
                    else if (mode === 4) win.setBox(stage.px(m.x) - gx, stage.py(m.y) - gy, win.cropW, win.cropH)
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
    }

    // ---- background preview
    Item {
        id: bgStage
        visible: edit.tool === "background"
        anchors { left: parent.left; right: parent.right; top: heading.bottom; bottom: controls.top; margins: 16 }
        readonly property real full: Math.max(1, win.imgSize.width + 2 * win.padding)
        readonly property real fullH: Math.max(1, win.imgSize.height + 2 * win.padding)
        readonly property real s: Math.min(width / full, height / fullH)
        readonly property var cols: win.swatches[win.swatch].split("-")
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
        visible: edit.tool === "trim"
        anchors { left: parent.left; right: parent.right; top: heading.bottom; bottom: controls.top; margins: 16 }
        Column {
            anchors.centerIn: parent
            width: Math.min(parent.width, 520)
            spacing: 18
            Text {
                text: edit.fileName
                color: win.ink
                font.pixelSize: 16
                font.weight: Font.DemiBold
                elide: Text.ElideMiddle
                width: parent.width
            }
            Text {
                text: win.total > 0 ? "Length " + win.clock(win.total) : "Length unknown"
                color: win.muted
                font.pixelSize: 13
            }
            // the part that is kept
            Rectangle {
                width: parent.width; height: 14; radius: 7
                color: win.dark ? "#2C3139" : "#DDE1E6"
                Rectangle {
                    readonly property real t: win.total > 0 ? win.total : Math.max(win.tEnd, 1)
                    visible: win.trimOk
                    x: parent.width * Math.max(0, Math.min(1, win.tStart / t))
                    width: Math.max(4, parent.width * Math.max(0, Math.min(1, (Math.min(win.tEnd, t) - win.tStart) / t)))
                    height: parent.height; radius: 7
                    color: win.accent
                }
            }
        }
    }

    // ---- controls
    Rectangle {
        id: controls
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: edit.tool === "background" ? 150 : edit.tool === "trim" ? 140 : 110
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
                    primary: Math.abs(win.ratio - modelData[1]) < 0.001
                    onClicked: win.pickRatio(modelData[1])
                }
            }
        }
        Text {
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
            Btn { text: "Undo last"; enabledState: boxes.count > 0; onClicked: boxes.remove(boxes.count - 1) }
            Btn { text: "Clear all"; enabledState: boxes.count > 0; onClicked: boxes.clear() }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: boxes.count === 0 ? "Nothing covered yet" : boxes.count + (boxes.count === 1 ? " part" : " parts") + " will be covered"
                color: win.muted
                font.pixelSize: 13
            }
        }
        Text {
            visible: edit.tool === "redact"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 230; topMargin: 56 }
            text: "The covered pixels are replaced in the copy, so the text under them can't be read back. The original is not changed."
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
                    readonly property var c: modelData.split("-")
                    border.width: win.swatch === index ? 3 : 1
                    border.color: win.swatch === index ? win.accent : win.line
                    gradient: Gradient {
                        GradientStop { position: 0; color: c[0] }
                        GradientStop { position: 1; color: c[c.length - 1] }
                    }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: win.swatch = index }
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: "Background " + (index + 1)
                    Accessible.checked: win.swatch === index
                    Accessible.onPressAction: win.swatch = index
                }
            }
        }
        Row {
            visible: edit.tool === "background"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 66 }
            spacing: 12
            Text { text: "Room around it"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            Slider {
                id: padSlider
                width: parent.width - 200
                from: 0; to: 300; value: win.padding
                onMoved: win.padding = value
                Accessible.name: "Room around the picture"
            }
            Text { text: Math.round(win.padding) + " px"; color: win.ink; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
        }

        // trim: two times
        Row {
            visible: edit.tool === "trim"
            anchors { left: parent.left; top: parent.top; margins: 16 }
            spacing: 14
            Text { text: "From"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TextField {
                id: startField
                width: 110
                text: "0:00"
                color: win.ink
                selectByMouse: true
                background: Rectangle { radius: 6; color: win.bg; border.color: startField.activeFocus ? win.accent : win.line }
                Accessible.name: "Start time"
            }
            Text { text: "to"; color: win.muted; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter }
            TextField {
                id: endField
                width: 110
                text: win.total > 0 ? win.clock(win.total) : ""
                color: win.ink
                selectByMouse: true
                background: Rectangle { radius: 6; color: win.bg; border.color: endField.activeFocus ? win.accent : win.line }
                Accessible.name: "End time"
            }
        }
        Text {
            visible: edit.tool === "trim"
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 16; rightMargin: 16; topMargin: 62 }
            text: win.trimOk ? "Keeps " + win.clock(Math.min(win.tEnd, win.total > 0 ? win.total : win.tEnd) - win.tStart)
                             : "Use seconds, mm:ss or hh:mm:ss. The end has to come after the start."
            color: win.trimOk ? win.muted : "#D6643A"
            font.pixelSize: 13
            wrapMode: Text.WordWrap
        }

        // buttons
        Row {
            anchors { right: parent.right; bottom: parent.bottom; margins: 16 }
            spacing: 10
            Btn { text: "Cancel"; onClicked: Qt.quit() }
            Btn {
                text: edit.tool === "crop" ? "Crop" : edit.tool === "redact" ? "Black out" : edit.tool === "background" ? "Add background" : "Trim"
                primary: true
                enabledState: win.canApply
                onClicked: win.apply()
            }
        }
    }
}
