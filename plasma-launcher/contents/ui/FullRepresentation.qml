/*
    The launcher popup: search on top, categories on the left, pinned apps or
    a category's apps on the right, and the account, privacy status and
    session buttons along the bottom.

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

Item {
    id: full

    readonly property int unit: Kirigami.Units.gridUnit
    Layout.preferredWidth: unit * 40
    Layout.preferredHeight: unit * 33
    Layout.minimumWidth: unit * 30
    Layout.minimumHeight: unit * 24

    // Row of root.rootModel shown on the right. Row 0 is the pinned apps.
    property int category: 0
    readonly property bool searching: searchField.text.length > 0

    // modelForRow() creates no binding dependency, so `refreshes` is bumped
    // whenever the root model rebuilds itself.
    property int refreshes: 0
    readonly property Kicker.AppsModel appsModel: {
        refreshes;
        return category > 0 ? root.rootModel.modelForRow(category) : null;
    }
    readonly property var searchModel: root.runnerModel.count ? root.runnerModel.modelForRow(0) : null
    readonly property Item contentView: searching ? searchView : (category === 0 ? pinnedView : appsView)

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
                full.category = 0;
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

    function openPinMenu(item, x, y, favoriteId) {
        if (!favoriteId) {
            return;
        }
        pinMenu.favoriteId = favoriteId;
        pinMenu.pinned = root.rootModel.favoritesModel.isFavorite(favoriteId);
        pinMenu.visualParent = item;
        pinMenu.open(x, y);
    }

    function focusContent() {
        const view = full.contentView;
        if (view.count > 0) {
            view.currentIndex = 0;
            view.forceActiveFocus(Qt.TabFocusReason);
        }
    }

    PlasmaExtras.Menu {
        id: pinMenu
        property string favoriteId: ""
        // isFavorite() is not a binding source, so it is read on each open.
        property bool pinned: false

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

        PlasmaExtras.SearchField {
            id: searchField

            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            Layout.bottomMargin: Kirigami.Units.smallSpacing
            implicitHeight: Math.round(full.unit * 2.4)
            placeholderText: i18nc("@info:placeholder", "Search apps and settings")
            focus: true

            onTextChanged: root.runnerModel.query = text

            function openFirstResult() {
                if (full.searching && searchView.count > 0) {
                    full.launch(full.searchModel, 0);
                }
            }

            Keys.onDownPressed: full.focusContent()
            Keys.onReturnPressed: openFirstResult()
            Keys.onEnterPressed: openFirstResult()
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            ListView {
                id: categories

                Layout.preferredWidth: full.unit * 10
                Layout.fillHeight: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                Layout.bottomMargin: Kirigami.Units.largeSpacing
                clip: true
                spacing: 2
                boundsBehavior: Flickable.StopAtBounds
                activeFocusOnTab: true
                keyNavigationWraps: false
                currentIndex: full.category
                highlightFollowsCurrentItem: false
                model: root.rootModel
                Accessible.name: i18nc("@label", "Categories")
                Accessible.role: Accessible.List

                Keys.onRightPressed: full.focusContent()
                Keys.onUpPressed: event => {
                    if (currentIndex <= 0) {
                        searchField.forceActiveFocus();
                    } else {
                        full.category = currentIndex - 1;
                    }
                }
                Keys.onDownPressed: {
                    if (currentIndex < count - 1) {
                        full.category = currentIndex + 1;
                    }
                }

                delegate: QQC2.AbstractButton {
                    id: categoryButton

                    required property int index
                    required property var model

                    readonly property bool current: !full.searching && full.category === index
                    readonly property int appCount: {
                        full.refreshes;
                        const model = index > 0 ? root.rootModel.modelForRow(index) : null;
                        return model ? model.count : 0;
                    }

                    width: ListView.view.width
                    height: Math.round(full.unit * 2)
                    hoverEnabled: true
                    focusPolicy: Qt.NoFocus
                    text: index === 0 ? i18nc("@item:inlistbox pinned apps", "Pinned") : model.display
                    Accessible.role: Accessible.ListItem
                    Accessible.selected: current

                    onClicked: {
                        searchField.text = "";
                        full.category = index;
                    }

                    background: Rectangle {
                        radius: Kirigami.Units.cornerRadius
                        color: categoryButton.current ? full.selectedBackground
                             : (categoryButton.hovered ? full.hoverBackground : "transparent")
                        border.width: categories.activeFocus && categoryButton.current ? 1 : 0
                        border.color: Kirigami.Theme.highlightColor
                    }

                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        PC3.Label {
                            Layout.fillWidth: true
                            Layout.leftMargin: Kirigami.Units.largeSpacing
                            text: categoryButton.text
                            elide: Text.ElideRight
                            font.weight: categoryButton.current ? Font.DemiBold : Font.Normal
                        }
                        PC3.Label {
                            Layout.rightMargin: Kirigami.Units.largeSpacing
                            visible: categoryButton.index > 0
                            text: categoryButton.appCount
                            opacity: 0.7
                            font: Kirigami.Theme.smallFont
                        }
                    }
                }
            }

            Kirigami.Separator {
                Layout.fillHeight: true
                Layout.bottomMargin: Kirigami.Units.largeSpacing
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 0

                PC3.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing * 2
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    Layout.bottomMargin: Kirigami.Units.smallSpacing
                    text: full.searching ? i18nc("@title", "Results")
                        : (full.category === 0 ? i18nc("@title pinned apps", "Pinned")
                        : root.rootModel.labelForRow(full.category))
                    font.weight: Font.DemiBold
                    opacity: 0.75
                    Accessible.role: Accessible.Heading
                }

                StackLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    currentIndex: full.searching ? 2 : (full.category === 0 ? 0 : 1)

                    GridView {
                        id: pinnedView

                        readonly property int columns: 4
                        clip: true
                        leftMargin: Kirigami.Units.largeSpacing
                        rightMargin: Kirigami.Units.largeSpacing
                        cellWidth: Math.floor((width - leftMargin - rightMargin) / columns)
                        cellHeight: Math.round(full.unit * 5.8)
                        boundsBehavior: Flickable.StopAtBounds
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
                        Keys.onLeftPressed: event => {
                            if (currentIndex % columns === 0) {
                                categories.forceActiveFocus();
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

                    ListView {
                        id: appsView

                        clip: true
                        spacing: 2
                        leftMargin: Kirigami.Units.largeSpacing
                        rightMargin: Kirigami.Units.largeSpacing
                        bottomMargin: Kirigami.Units.largeSpacing
                        boundsBehavior: Flickable.StopAtBounds
                        activeFocusOnTab: true
                        model: full.appsModel
                        Accessible.role: Accessible.List
                        PC3.ScrollBar.vertical: PC3.ScrollBar {}

                        Keys.onReturnPressed: full.launch(model, currentIndex)
                        Keys.onEnterPressed: full.launch(model, currentIndex)
                        Keys.onLeftPressed: categories.forceActiveFocus()
                        Keys.onUpPressed: event => {
                            if (currentIndex <= 0) {
                                searchField.forceActiveFocus();
                            } else {
                                event.accepted = false;
                            }
                        }

                        delegate: AppRow {
                            view: appsView
                        }
                    }

                    ListView {
                        id: searchView

                        clip: true
                        spacing: 2
                        leftMargin: Kirigami.Units.largeSpacing
                        rightMargin: Kirigami.Units.largeSpacing
                        bottomMargin: Kirigami.Units.largeSpacing
                        boundsBehavior: Flickable.StopAtBounds
                        activeFocusOnTab: true
                        model: full.searchModel
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
                            // Enter in the search field opens the first result,
                            // so it is marked while typing.
                            marked: index === 0 && searchField.activeFocus
                        }

                        PlasmaExtras.PlaceholderMessage {
                            anchors.centerIn: parent
                            width: parent.width - full.unit * 4
                            visible: full.searching && !root.runnerModel.querying && searchView.count === 0
                            text: i18nc("@info:status", "No apps or settings match")
                        }
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

            // The account picture, or the first letter of the name.
            Rectangle {
                readonly property string name: kuser.fullName || kuser.loginName

                Layout.preferredWidth: Math.round(full.unit * 1.8)
                Layout.preferredHeight: Layout.preferredWidth
                radius: width / 2
                color: Qt.alpha(Kirigami.Theme.highlightColor, 0.18)
                Accessible.ignored: true

                PC3.Label {
                    anchors.centerIn: parent
                    visible: face.status !== Image.Ready
                    text: parent.name.charAt(0).toUpperCase()
                    font.weight: Font.DemiBold
                }
                Kirigami.ShadowedImage {
                    id: face
                    anchors.fill: parent
                    source: kuser.faceIconUrl
                    radius: width / 2
                    fillMode: Image.PreserveAspectCrop
                    visible: status === Image.Ready
                }
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

                background: Rectangle {
                    radius: height / 2
                    color: privacyChip.hovered ? full.hoverBackground : "transparent"
                    border.width: 1
                    border.color: privacyChip.visualFocus ? Kirigami.Theme.highlightColor
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
            onClicked: mouse => full.openPinMenu(tile, mouse.x, mouse.y, tile.model.favoriteId)
        }

        background: Rectangle {
            anchors.fill: parent
            anchors.margins: 2
            radius: Kirigami.Units.cornerRadius * 2
            color: (tile.hovered || (tile.view.activeFocus && tile.view.currentIndex === tile.index)) ? full.hoverBackground : "transparent"
            border.width: tile.view.activeFocus && tile.view.currentIndex === tile.index ? 1 : 0
            border.color: Kirigami.Theme.highlightColor
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
            }
            Item {
                Layout.fillHeight: true
            }
        }
    }

    // An app or setting in a list: icon, name, and what it is.
    component AppRow: QQC2.AbstractButton {
        id: row

        required property int index
        required property var model
        property ListView view
        property bool marked: false

        readonly property string details: model.description ?? ""

        width: view.width - view.leftMargin - view.rightMargin
        height: Math.round(full.unit * 2.9)
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
            onClicked: mouse => full.openPinMenu(row, mouse.x, mouse.y, row.model.favoriteId)
        }

        background: Rectangle {
            radius: Kirigami.Units.cornerRadius * 2
            color: (row.hovered || row.marked || (row.view.activeFocus && row.view.currentIndex === row.index)) ? full.hoverBackground : "transparent"
            border.width: row.view.activeFocus && row.view.currentIndex === row.index ? 1 : 0
            border.color: Kirigami.Theme.highlightColor
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
        }
    }

    Component.onCompleted: {
        root.rootModel.refresh();
        privacy.refresh();
    }
}
