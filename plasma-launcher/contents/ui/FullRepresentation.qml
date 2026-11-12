/*
    The launcher popup: search on top, then the pinned apps, then all apps as an
    A-Z list or a grid, and the account, privacy status and session buttons along
    the bottom. There are no categories: people search or scroll the A-Z list, and
    a category list sorted apps into groups nobody looked in (launcher research,
    2026-11-12).

    Typing searches apps and settings on this computer only. Ctrl+1 to Ctrl+9
    open the first nine results; "=" works out a sum, here, without the network.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.components as PC3
import org.kde.plasma.extras as PlasmaExtras
import org.kde.plasma.plasma5support as P5Support
import org.kde.plasma.private.kicker as Kicker
import org.kde.plasma.private.sessions as Sessions
import org.kde.kirigami as Kirigami
import org.kde.coreaddons as KCoreAddons

import "Calc.js" as Calc

Item {
    id: full

    // Typing while a grid, list or button has focus still searches: the key goes to the search field.
    Keys.onPressed: event => {
        if (!searchField.activeFocus && event.text.length > 0 && event.text.trim().length > 0
                && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
            searchField.forceActiveFocus();
            searchField.insert(searchField.cursorPosition, event.text);
            event.accepted = true;
        }
    }

    readonly property int unit: Kirigami.Units.gridUnit
    Layout.preferredWidth: unit * 36
    Layout.preferredHeight: unit * 38
    Layout.minimumWidth: unit * 28
    Layout.minimumHeight: unit * 26

    readonly property bool searching: searchField.text.length > 0
    // "=12*8", or a bare "12*8" (digits and operators only, so never an app name): a sum, worked out
    // here. Null when the text is not one.
    readonly property bool sumAsked: searchField.text.startsWith("=")
    readonly property var sum: sumAsked ? Calc.evaluate(searchField.text.slice(1))
        : /[0-9]/.test(searchField.text) && /[0-9.)]\s*[-+*\/%^×÷]/.test(searchField.text) ? Calc.evaluate(searchField.text) : null
    readonly property bool asGrid: Plasmoid.configuration.allAppsAsGrid

    // modelForRow() creates no binding dependency, so `refreshes` is bumped
    // whenever the root model rebuilds itself. Row 1 is "All Applications".
    property int refreshes: 0
    readonly property Kicker.AppsModel allAppsModel: {
        refreshes;
        return root.rootModel.count > 1 ? root.rootModel.modelForRow(1) : null;
    }
    readonly property var searchModel: root.runnerModel.count ? root.runnerModel.modelForRow(0) : null

    // Highlights, tiles and fields share the popups' cut corners (CutRect).
    readonly property real cut: Math.round(Kirigami.Units.smallSpacing * 1.5)
    readonly property color selectedBackground: Qt.alpha(Kirigami.Theme.highlightColor, 0.16)
    readonly property color hoverBackground: Qt.alpha(Kirigami.Theme.textColor, 0.06)

    Connections {
        target: root.rootModel
        function onRefreshed() {
            full.refreshes += 1;
        }
    }

    Connections {
        target: root
        function onExpandedChanged() {
            if (root.expanded) {
                searchField.text = "";
                scroller.contentItem.contentY = 0;
                searchField.forceActiveFocus();
                privacy.refresh();
            }
        }
    }

    function launch(model, index) {
        if (!model || index < 0) {
            return;
        }
        model.trigger(index, "", null);
        root.expanded = false;
    }

    function openPinMenu(item, x, y, favoriteId, index, fromPinned) {
        if (!favoriteId) {
            return;
        }
        pinMenu.favoriteId = favoriteId;
        pinMenu.index = index;
        pinMenu.fromPinned = fromPinned === true;
        pinMenu.pinned = root.rootModel.favoritesModel.isFavorite(favoriteId);
        pinMenu.visualParent = item;
        pinMenu.open(x, y);
    }

    function focusFirst(view) {
        if (view && view.count > 0) {
            view.currentIndex = 0;
            view.forceActiveFocus(Qt.TabFocusReason);
        }
    }

    PlasmaExtras.Menu {
        id: pinMenu
        property string favoriteId: ""
        // isFavorite() is not a binding source, so it is read on each open.
        property bool pinned: false
        // Where the pin sits in the pinned list, when the menu came from there.
        property int index: -1
        property bool fromPinned: false

        PlasmaExtras.MenuItem {
            text: pinMenu.pinned ? i18nc("@action:inmenu", "Unpin") : i18nc("@action:inmenu", "Pin")
            icon: pinMenu.pinned ? "window-unpin" : "window-pin"
            onClicked: {
                const favorites = root.rootModel.favoritesModel;
                if (pinMenu.pinned) {
                    favorites.removeFavorite(pinMenu.favoriteId);
                } else {
                    favorites.addFavorite(pinMenu.favoriteId, -1);
                }
            }
        }
        PlasmaExtras.MenuItem {
            text: i18nc("@action:inmenu", "Move earlier")
            icon: "go-previous"
            visible: pinMenu.pinned && pinMenu.fromPinned
            enabled: pinMenu.index > 0
            onClicked: root.rootModel.favoritesModel.moveRow(pinMenu.index, pinMenu.index - 1)
        }
        PlasmaExtras.MenuItem {
            text: i18nc("@action:inmenu", "Move later")
            icon: "go-next"
            visible: pinMenu.pinned && pinMenu.fromPinned
            enabled: pinMenu.index >= 0 && pinMenu.index < pinnedView.count - 1
            onClicked: root.rootModel.favoritesModel.moveRow(pinMenu.index, pinMenu.index + 1)
        }
    }

    // The privacy line is the tray's own text: arka-privacy-tray --status
    // prints the same level and header the tray shows, from one fresh reading.
    P5Support.DataSource {
        id: privacy

        readonly property string statusCommand: "arka-privacy-tray --status"
        property string level: "unknown"
        property string header: ""

        engine: "executable"
        connectedSources: []

        function refresh() {
            disconnectSource(statusCommand);
            connectSource(statusCommand);
        }

        function run(command) {
            disconnectSource(command);
            connectSource(command);
        }

        onNewData: (source, data) => {
            if (source === statusCommand) {
                const line = (data["stdout"] || "").trim();
                const tab = line.indexOf("\t");
                if (data["exit code"] === 0 && tab > 0) {
                    level = line.slice(0, tab);
                    header = line.slice(tab + 1);
                } else {
                    level = "unknown";
                    header = "";
                }
            }
            disconnectSource(source);
        }
    }

    Sessions.SessionManagement {
        id: session
    }

    KCoreAddons.KUser {
        id: kuser
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // The search field with the popups' cut corners. The frame is drawn
        // behind the field rather than as its background: Plasma's TextField
        // sizes itself from its background, which loops with a Shape.
        Item {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            Layout.bottomMargin: Kirigami.Units.smallSpacing
            implicitHeight: Math.round(full.unit * 2.4)

            CutRect {
                anchors.fill: parent
                cut: full.cut
                color: Qt.alpha(Kirigami.Theme.textColor, 0.04)
                borderWidth: 1
                borderColor: searchField.activeFocus ? Kirigami.Theme.highlightColor
                    : Qt.alpha(Kirigami.Theme.textColor, 0.18)
            }

            PlasmaExtras.SearchField {
                id: searchField

                anchors.fill: parent
                placeholderText: i18nc("@info:placeholder", "Search apps and settings")
                // Plasma's default placeholder measured 3.4:1; this keeps it clearly a hint but readable.
                placeholderTextColor: Qt.alpha(Kirigami.Theme.textColor, 0.72)
                focus: true
                background: null
                // Without Plasma's frame there are no frame margins, so keep a
                // gap between the search icon and the text.
                leftPadding: effectiveHorizontalAlignment === TextInput.AlignRight
                    ? Kirigami.Units.largeSpacing : _iconWidth + Kirigami.Units.largeSpacing
                rightPadding: effectiveHorizontalAlignment === TextInput.AlignRight
                    ? _iconWidth + Kirigami.Units.largeSpacing : Kirigami.Units.largeSpacing

                // A sum is not sent to the search: only "=" text is worked out, here.
                onTextChanged: root.runnerModel.query = text.startsWith("=") ? "" : text

                function openResult(n) {
                    if (full.searching && n < searchView.count) {
                        full.launch(full.searchModel, n);
                    }
                }

                Keys.onDownPressed: full.searching ? full.focusFirst(searchView) : full.focusFirst(pinnedView)
                Keys.onReturnPressed: openResult(0)
                Keys.onEnterPressed: openResult(0)
                // Ctrl+1 .. Ctrl+9 open that result (plain digits are part of what you type).
                Keys.onPressed: event => {
                    if ((event.modifiers & Qt.ControlModifier) && event.key >= Qt.Key_1 && event.key <= Qt.Key_9) {
                        openResult(event.key - Qt.Key_1);
                        event.accepted = true;
                    }
                }
            }
        }

        // --- While typing: the sum, or the results ---------------------------
        ColumnLayout {
            visible: full.searching
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // "=12*8" -> 96, worked out here. Enter copies it.
            QQC2.AbstractButton {
                id: sumRow
                visible: full.sumAsked || full.sum !== null
                Layout.fillWidth: true
                implicitWidth: full.unit * 20
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                implicitHeight: Math.round(full.unit * 3.2)
                text: full.sum !== null ? Calc.format(full.sum) : i18nc("@info", "Type a sum, for example =12*8")
                Accessible.name: full.sum !== null ? i18nc("@info %1 is a number", "Answer %1. Press to copy it", text) : text
                onClicked: if (full.sum !== null) { clip.text = text; clip.selectAll(); clip.copy(); root.expanded = false; }
                background: CutRect {
                    cut: full.cut
                    color: full.hoverBackground
                }
                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    PC3.Label {
                        Layout.leftMargin: Kirigami.Units.largeSpacing
                        text: "="
                        font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.6
                        font.weight: Font.DemiBold
                        opacity: 0.75
                    }
                    PC3.Label {
                        Layout.fillWidth: true
                        text: sumRow.text
                        font.pointSize: full.sum !== null ? Kirigami.Theme.defaultFont.pointSize * 1.6 : Kirigami.Theme.defaultFont.pointSize
                        opacity: full.sum !== null ? 1 : 0.75
                        elide: Text.ElideRight
                    }
                    PC3.Label {
                        visible: full.sum !== null
                        Layout.rightMargin: Kirigami.Units.largeSpacing
                        text: i18nc("@info pressing Enter copies the answer", "Enter copies")
                        font: Kirigami.Theme.smallFont
                        opacity: 0.75
                    }
                }
                TextEdit { id: clip; visible: false }
                Connections {
                    target: searchField
                    function onAccepted() {
                        if (sumRow.visible) {
                            sumRow.clicked();
                        }
                    }
                }
            }

            ListView {
                id: searchView

                visible: !searchField.text.startsWith("=")
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 2
                leftMargin: Kirigami.Units.largeSpacing
                rightMargin: Kirigami.Units.largeSpacing
                boundsBehavior: Flickable.StopAtBounds
                activeFocusOnTab: true
                model: full.searchModel
                Accessible.name: i18nc("@label", "Results")
                Accessible.role: Accessible.List
                PC3.ScrollBar.vertical: PC3.ScrollBar {}

                Keys.onReturnPressed: full.launch(model, currentIndex)
                Keys.onEnterPressed: full.launch(model, currentIndex)
                Keys.onUpPressed: event => {
                    if (currentIndex <= 0) {
                        searchField.forceActiveFocus();
                    } else {
                        event.accepted = false;
                    }
                }

                delegate: AppRow {
                    view: searchView
                    // Enter in the search field opens the first result, so it is marked while typing.
                    marked: index === 0 && searchField.activeFocus
                    keyHint: index === 0 ? i18nc("@info keyboard key", "Enter")
                        : index < 9 ? i18nc("@info keyboard shortcut, %1 is a digit", "Ctrl+%1", index + 1) : ""
                }

                PlasmaExtras.PlaceholderMessage {
                    anchors.centerIn: parent
                    width: parent.width - full.unit * 4
                    visible: full.searching && !root.runnerModel.querying && searchView.count === 0 && !sumRow.visible
                    text: i18nc("@info:status", "No apps or settings match")
                }
            }

            // Said where it is true: the launcher's search runs only the apps and settings runners.
            PC3.Label {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                text: i18nc("@info", "Searches apps and settings on this computer. Nothing you type is sent online.")
                font: Kirigami.Theme.smallFont
                opacity: 0.75
                wrapMode: Text.Wrap
            }
        }

        // --- Not typing: pinned apps, then all apps ---------------------------
        PC3.ScrollView {
            id: scroller

            visible: !full.searching
            Layout.fillWidth: true
            Layout.fillHeight: true
            PC3.ScrollBar.horizontal.policy: PC3.ScrollBar.AlwaysOff

            ColumnLayout {
                width: scroller.availableWidth
                spacing: 0

                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing * 2
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    PC3.Label {
                        Layout.fillWidth: true
                        text: i18nc("@title pinned apps", "Pinned")
                        font.weight: Font.DemiBold
                        Accessible.role: Accessible.Heading
                    }
                    PC3.ToolButton {
                        text: Plasmoid.configuration.showRecent ? i18nc("@action:button", "Hide recent") : i18nc("@action:button", "Show recent")
                        PC3.ToolTip.text: i18nc("@info:tooltip", "Recent apps are kept only on this computer")
                        PC3.ToolTip.visible: hovered
                        PC3.ToolTip.delay: Kirigami.Units.toolTipDelay
                        onClicked: Plasmoid.configuration.showRecent = !Plasmoid.configuration.showRecent
                    }
                }

                GridView {
                    id: pinnedView

                    readonly property int columns: 6
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.max(1, Math.ceil(count / columns)) * cellHeight
                    interactive: false
                    leftMargin: Kirigami.Units.largeSpacing
                    rightMargin: Kirigami.Units.largeSpacing
                    cellWidth: Math.floor((width - leftMargin - rightMargin) / columns)
                    cellHeight: Math.round(full.unit * 5.2)
                    keyNavigationWraps: false
                    activeFocusOnTab: true
                    model: root.rootModel.favoritesModel
                    Accessible.name: i18nc("@label", "Pinned apps")
                    Accessible.role: Accessible.List

                    Keys.onReturnPressed: full.launch(model, currentIndex)
                    Keys.onEnterPressed: full.launch(model, currentIndex)
                    Keys.onUpPressed: event => {
                        if (currentIndex < columns) {
                            searchField.forceActiveFocus();
                        } else {
                            event.accepted = false;
                        }
                    }
                    Keys.onDownPressed: event => {
                        if (currentIndex + columns >= count) {
                            full.focusFirst(Plasmoid.configuration.showRecent && recentView.count > 0 ? recentView
                                : (full.asGrid ? allGrid : allList));
                        } else {
                            event.accepted = false;
                        }
                    }

                    delegate: AppTile {
                        view: pinnedView
                    }

                    PlasmaExtras.PlaceholderMessage {
                        anchors.centerIn: parent
                        width: parent.width - full.unit * 4
                        visible: pinnedView.count === 0
                        text: i18nc("@info", "No pinned apps")
                        explanation: i18nc("@info", "Right-click an app and choose Pin.")
                    }
                }

                // Recent apps: off until the user turns them on, and only kept on this computer.
                RowLayout {
                    visible: Plasmoid.configuration.showRecent
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing * 2
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    PC3.Label {
                        Layout.fillWidth: true
                        text: i18nc("@title recently opened apps", "Recent")
                        font.weight: Font.DemiBold
                        Accessible.role: Accessible.Heading
                    }
                    PC3.ToolButton {
                        visible: recentView.count > 0
                        text: i18nc("@action:button forget the recent apps", "Clear")
                        PC3.ToolTip.text: i18nc("@info:tooltip", "Forget which apps you opened recently")
                        PC3.ToolTip.visible: hovered
                        PC3.ToolTip.delay: Kirigami.Units.toolTipDelay
                        onClicked: root.recentModel.trigger(0, "_kicker_forgetAll", null)
                    }
                }
                GridView {
                    id: recentView

                    visible: Plasmoid.configuration.showRecent && count > 0
                    interactive: false
                    Layout.fillWidth: true
                    Layout.preferredHeight: visible ? cellHeight : 0
                    leftMargin: Kirigami.Units.largeSpacing
                    rightMargin: Kirigami.Units.largeSpacing
                    cellWidth: Math.floor((width - leftMargin - rightMargin) / 6)
                    cellHeight: Math.round(full.unit * 5.2)
                    activeFocusOnTab: true
                    keyNavigationWraps: false
                    model: root.recentModel
                    Accessible.name: i18nc("@label", "Recent apps")
                    Accessible.role: Accessible.List
                    // One row of six is shown.
                    onCurrentIndexChanged: if (currentIndex > 5) currentIndex = 5
                    Keys.onReturnPressed: full.launch(model, currentIndex)
                    Keys.onEnterPressed: full.launch(model, currentIndex)
                    delegate: AppTile {
                        view: recentView
                    }
                }

                Kirigami.Separator {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing * 2
                    Layout.rightMargin: Kirigami.Units.largeSpacing * 2
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    Layout.bottomMargin: Kirigami.Units.smallSpacing
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing * 2
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    PC3.Label {
                        Layout.fillWidth: true
                        text: i18nc("@title", "All apps")
                        font.weight: Font.DemiBold
                        Accessible.role: Accessible.Heading
                    }
                    // Grid or A-Z: a choice, remembered (people asked for both; launcher research).
                    PC3.ToolButton {
                        text: i18nc("@action:button show all apps as a grid", "Grid")
                        checkable: true
                        checked: full.asGrid
                        onClicked: Plasmoid.configuration.allAppsAsGrid = true
                        Keys.onReturnPressed: clicked()
                        Keys.onEnterPressed: clicked()
                    }
                    PC3.ToolButton {
                        text: i18nc("@action:button show all apps as an alphabetical list", "A–Z")
                        checkable: true
                        checked: !full.asGrid
                        onClicked: Plasmoid.configuration.allAppsAsGrid = false
                        Keys.onReturnPressed: clicked()
                        Keys.onEnterPressed: clicked()
                    }
                }

                ListView {
                    id: allList

                    visible: !full.asGrid
                    Layout.fillWidth: true
                    Layout.preferredHeight: contentHeight
                    interactive: false
                    spacing: 2
                    leftMargin: Kirigami.Units.largeSpacing
                    rightMargin: Kirigami.Units.largeSpacing
                    activeFocusOnTab: true
                    model: full.allAppsModel
                    Accessible.name: i18nc("@label", "All apps")
                    Accessible.role: Accessible.List

                    section.property: "display"
                    section.criteria: ViewSection.FirstCharacter
                    section.delegate: PC3.Label {
                        required property string section
                        width: ListView.view.width
                        leftPadding: Kirigami.Units.largeSpacing * 2
                        topPadding: Kirigami.Units.smallSpacing
                        text: section.toUpperCase()
                        font.weight: Font.Bold
                        // Text colour, not the accent: the light look's green is about 2.3:1 on the popup.
                        Accessible.role: Accessible.Heading
                    }

                    Keys.onReturnPressed: full.launch(model, currentIndex)
                    Keys.onEnterPressed: full.launch(model, currentIndex)
                    // The list sits in a scroll view, so keep the current row in sight.
                    onCurrentItemChanged: if (currentItem && activeFocus) scroller.contentItem.contentY =
                        Math.max(0, Math.min(currentItem.mapToItem(scroller.contentItem.contentItem, 0, 0).y - full.unit * 4,
                                             scroller.contentItem.contentHeight - scroller.height))

                    delegate: AppRow {
                        view: allList
                    }
                }

                GridView {
                    id: allGrid

                    visible: full.asGrid
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.max(1, Math.ceil(count / 6)) * cellHeight
                    interactive: false
                    leftMargin: Kirigami.Units.largeSpacing
                    rightMargin: Kirigami.Units.largeSpacing
                    cellWidth: Math.floor((width - leftMargin - rightMargin) / 6)
                    cellHeight: Math.round(full.unit * 5.2)
                    activeFocusOnTab: true
                    keyNavigationWraps: false
                    model: full.allAppsModel
                    Accessible.name: i18nc("@label", "All apps")
                    Accessible.role: Accessible.List
                    Keys.onReturnPressed: full.launch(model, currentIndex)
                    Keys.onEnterPressed: full.launch(model, currentIndex)
                    onCurrentItemChanged: if (currentItem && activeFocus) scroller.contentItem.contentY =
                        Math.max(0, Math.min(currentItem.mapToItem(scroller.contentItem.contentItem, 0, 0).y - full.unit * 4,
                                             scroller.contentItem.contentHeight - scroller.height))
                    delegate: AppTile {
                        view: allGrid
                    }
                }
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            Layout.topMargin: Kirigami.Units.smallSpacing
            Layout.bottomMargin: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.largeSpacing

            // The account picture, only when there is one. No placeholder disc with a letter.
            Kirigami.ShadowedImage {
                id: face
                Layout.preferredWidth: Math.round(full.unit * 1.8)
                Layout.preferredHeight: Layout.preferredWidth
                source: kuser.faceIconUrl
                radius: width / 2
                fillMode: Image.PreserveAspectCrop
                visible: status === Image.Ready
                Accessible.ignored: true
            }

            PC3.Label {
                text: kuser.fullName || kuser.loginName
                elide: Text.ElideRight
                Layout.maximumWidth: full.unit * 8
            }

            QQC2.AbstractButton {
                id: privacyChip

                readonly property color dotColor: privacy.level === "ok" ? Kirigami.Theme.positiveTextColor
                    : privacy.level === "warn" ? Kirigami.Theme.neutralTextColor
                    : privacy.level === "critical" ? Kirigami.Theme.negativeTextColor
                    : Kirigami.Theme.disabledTextColor

                Layout.fillWidth: true
                Layout.maximumWidth: implicitWidth
                implicitHeight: Math.round(full.unit * 1.8)
                implicitWidth: chipRow.implicitWidth + Kirigami.Units.largeSpacing * 2
                hoverEnabled: true
                text: privacy.header !== "" ? privacy.header : i18nc("@action:button", "Privacy Dashboard")
                Accessible.name: privacy.header !== ""
                    ? i18nc("@action:button %1 is the privacy status", "Privacy status: %1. Open the Privacy Dashboard", privacy.header)
                    : i18nc("@action:button", "Open the Privacy Dashboard")
                PC3.ToolTip.text: i18nc("@info:tooltip", "Open the Privacy Dashboard")
                PC3.ToolTip.visible: hovered && contentLabel.truncated
                PC3.ToolTip.delay: Kirigami.Units.toolTipDelay

                onClicked: {
                    privacy.run("kstart --application arka-dashboard");
                    root.expanded = false;
                }

                background: CutRect {
                    cut: full.cut
                    color: privacyChip.hovered ? full.hoverBackground : "transparent"
                    borderWidth: 1
                    borderColor: privacyChip.visualFocus ? Kirigami.Theme.highlightColor
                        : Qt.alpha(Kirigami.Theme.textColor, 0.15)
                }

                contentItem: RowLayout {
                    id: chipRow
                    spacing: Kirigami.Units.smallSpacing
                    Rectangle {
                        Layout.leftMargin: Kirigami.Units.largeSpacing
                        implicitWidth: Kirigami.Units.smallSpacing * 2
                        implicitHeight: implicitWidth
                        radius: width / 2
                        color: privacyChip.dotColor
                    }
                    PC3.Label {
                        id: contentLabel
                        Layout.fillWidth: true
                        Layout.rightMargin: Kirigami.Units.largeSpacing
                        text: privacyChip.text
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
                    }
                }
            }

            Item {
                Layout.fillWidth: true
            }

            PC3.ToolButton {
                visible: session.canLock
                icon.name: "system-lock-screen"
                display: PC3.AbstractButton.IconOnly
                text: i18nc("@action:button", "Lock")
                PC3.ToolTip.text: text
                PC3.ToolTip.visible: hovered
                PC3.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    root.expanded = false;
                    session.lock();
                }
            }
            PC3.ToolButton {
                visible: session.canLogout
                icon.name: "system-log-out"
                display: PC3.AbstractButton.IconOnly
                text: i18nc("@action:button", "Log Out")
                PC3.ToolTip.text: text
                PC3.ToolTip.visible: hovered
                PC3.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    root.expanded = false;
                    session.requestLogout();
                }
            }
            PC3.ToolButton {
                visible: session.canReboot
                icon.name: "system-reboot"
                display: PC3.AbstractButton.IconOnly
                text: i18nc("@action:button", "Restart")
                PC3.ToolTip.text: text
                PC3.ToolTip.visible: hovered
                PC3.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    root.expanded = false;
                    session.requestReboot();
                }
            }
            PC3.ToolButton {
                visible: session.canShutdown
                icon.name: "system-shutdown"
                display: PC3.AbstractButton.IconOnly
                text: i18nc("@action:button", "Shut Down")
                PC3.ToolTip.text: text
                PC3.ToolTip.visible: hovered
                PC3.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    root.expanded = false;
                    session.requestShutdown();
                }
            }
        }
    }

    // A pinned app: large icon over its name.
    component AppTile: QQC2.AbstractButton {
        id: tile

        required property int index
        required property var model
        property GridView view

        width: view.cellWidth
        height: view.cellHeight
        hoverEnabled: true
        focusPolicy: Qt.NoFocus
        text: model.display
        Accessible.role: Accessible.ListItem
        Accessible.selected: view.activeFocus && view.currentIndex === index

        onClicked: full.launch(view.model, index)

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.RightButton
            onClicked: mouse => full.openPinMenu(tile, mouse.x, mouse.y, tile.model.favoriteId, tile.index, tile.view === pinnedView)
        }

        background: CutRect {
            anchors.fill: parent
            anchors.margins: 2
            cut: full.cut + 2
            color: (tile.hovered || (tile.view.activeFocus && tile.view.currentIndex === tile.index)) ? full.hoverBackground : "transparent"
            borderWidth: tile.view.activeFocus && tile.view.currentIndex === tile.index ? 1 : 0
            borderColor: Kirigami.Theme.highlightColor
        }

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Item {
                Layout.fillHeight: true
            }
            Kirigami.Icon {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: Kirigami.Units.iconSizes.large
                Layout.preferredHeight: Kirigami.Units.iconSizes.large
                source: tile.model.decoration
            }
            PC3.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.smallSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                text: tile.model.display
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                maximumLineCount: 2
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }
            Item {
                Layout.fillHeight: true
            }
        }
    }

    // An app or setting in a list: icon, name, what it is, and its key when it has one.
    component AppRow: QQC2.AbstractButton {
        id: row

        required property int index
        required property var model
        property ListView view
        property bool marked: false
        property string keyHint: ""

        readonly property string details: model.description ?? ""

        width: view.width - view.leftMargin - view.rightMargin
        height: Math.round(full.unit * 2.7)
        hoverEnabled: true
        focusPolicy: Qt.NoFocus
        text: model.display
        Accessible.role: Accessible.ListItem
        Accessible.description: details !== model.display ? details : ""
        Accessible.selected: view.activeFocus && view.currentIndex === index

        onClicked: full.launch(view.model, index)

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.RightButton
            onClicked: mouse => full.openPinMenu(row, mouse.x, mouse.y, row.model.favoriteId, -1, false)
        }

        background: CutRect {
            cut: full.cut
            color: (row.hovered || row.marked || (row.view.activeFocus && row.view.currentIndex === row.index)) ? full.hoverBackground : "transparent"
            borderWidth: row.view.activeFocus && row.view.currentIndex === row.index ? 1 : 0
            borderColor: Kirigami.Theme.highlightColor
        }

        contentItem: RowLayout {
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                Layout.leftMargin: Kirigami.Units.smallSpacing
                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                source: row.model.decoration
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                PC3.Label {
                    Layout.fillWidth: true
                    text: row.model.display
                    elide: Text.ElideRight
                    font.weight: Font.Medium
                }
                PC3.Label {
                    Layout.fillWidth: true
                    visible: text !== "" && text !== row.model.display
                    text: row.details
                    elide: Text.ElideRight
                    font: Kirigami.Theme.smallFont
                    opacity: 0.75
                }
            }
            // The key that opens it: "Enter" for the first result, Ctrl+2 .. Ctrl+9 for the next.
            PC3.Label {
                visible: row.keyHint !== ""
                Layout.rightMargin: Kirigami.Units.smallSpacing
                text: row.keyHint
                font: Kirigami.Theme.smallFont
                opacity: 0.75
                leftPadding: Kirigami.Units.smallSpacing
                rightPadding: Kirigami.Units.smallSpacing
                background: Rectangle {
                    radius: 3
                    color: Qt.alpha(Kirigami.Theme.textColor, 0.08)
                }
            }
        }
    }

    Component.onCompleted: {
        root.rootModel.refresh();
        privacy.refresh();
    }
}
