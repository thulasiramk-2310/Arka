// ArkaOS Dock: pinned and open apps in one row. Icons grow under the pointer,
// a dot marks running apps (accent colour for the active one), an app bounces
// while it starts. Built on Plasma's own task model, so windows, pinning and
// activation behave exactly like Plasma's task manager. The dock is always
// visible unless "Hide dock automatically" is ticked (right-click).
import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.extras as PlasmaExtras
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami
import org.kde.taskmanager as TaskManager

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation
    Plasmoid.constraintHints: Plasmoid.CanFillArea

    // Icon size at rest, leaving room inside the panel for the zoom and the dot.
    readonly property real dotSpace: Math.max(6, Math.round(root.height * 0.1))
    readonly property real restSize: Math.max(16, Math.round((root.height - dotSpace) / maxZoom))
    readonly property real maxZoom: Plasmoid.configuration.magnify ? 1.45 : 1.0
    readonly property real gap: Math.round(restSize * 0.18)
    readonly property real slot: restSize + gap
    readonly property real restWidth: Math.max(1, tasksModel.count) * slot
    // Pointer position in "rest" coordinates (as if nothing were zoomed), or
    // -1 when the pointer is not over the dock. Zoom depends on this, not on
    // the moving icons, so the row doesn't feed back on itself.
    readonly property real pointerRest: tracker.hovered
        ? tracker.point.position.x - (root.width - root.restWidth) / 2 : -1

    // Fixed width: the dock never resizes the panel while zooming.
    Layout.preferredWidth: restWidth + restSize * (maxZoom - 1) * 2.2 + gap
    Layout.minimumWidth: Layout.preferredWidth
    Layout.maximumWidth: Layout.preferredWidth
    Layout.fillHeight: true

    // The panel's own visibility setting (the same one as in Edit Mode)
    // follows Plasmoid.configuration.autoHide. A widget can't change its
    // panel directly, so ask Plasma through its scripting interface.
    function applyAutoHide() {
        const id = Plasmoid.containment ? Plasmoid.containment.id : -1;
        if (id < 0) return;
        const mode = Plasmoid.configuration.autoHide ? "autohide" : "none";
        runner.connectSource("dbus-send --session --type=method_call --dest=org.kde.plasmashell"
            + " /PlasmaShell org.kde.PlasmaShell.evaluateScript"
            + " string:\"panelById(" + id + ").hiding = '" + mode + "'\"");
    }

    function setAutoHide(on) {
        Plasmoid.configuration.autoHide = on;
        applyAutoHide();
    }

    P5Support.DataSource {
        id: runner
        engine: "executable"
        onNewData: source => disconnectSource(source)
    }

    Component.onCompleted: if (Plasmoid.configuration.autoHide) applyAutoHide()

    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: i18n("Hide dock automatically")
            checkable: true
            checked: Plasmoid.configuration.autoHide
            onTriggered: root.setAutoHide(checked)
        }
    ]

    function zoomAt(restCenter) {
        if (pointerRest < 0 || maxZoom <= 1) return 1;
        const d = (restCenter - pointerRest) / (restSize * 1.25);
        return 1 + (maxZoom - 1) * Math.exp(-0.5 * d * d);
    }

    TaskManager.ActivityInfo { id: activityInfo }
    TaskManager.VirtualDesktopInfo { id: desktopInfo }

    TaskManager.TasksModel {
        id: tasksModel
        sortMode: TaskManager.TasksModel.SortManual
        groupMode: TaskManager.TasksModel.GroupApplications
        separateLaunchers: false
        launchInPlace: true
        hideActivatedLaunchers: true
        filterByVirtualDesktop: true
        virtualDesktop: desktopInfo.currentDesktop
        filterByActivity: true
        activity: activityInfo.currentActivity
        filterByScreen: false
        filterHidden: false   // minimised windows stay in the dock, with their dot
        launcherList: Plasmoid.configuration.launchers
        onLauncherListChanged: Plasmoid.configuration.launchers = launcherList
        Component.onCompleted: countChanged()
    }

    // Sees the pointer even over the icons' own mouse areas, and reliably
    // reports leaving, so the zoom always settles back.
    HoverHandler { id: tracker }

    Row {
        id: row
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        spacing: 0

        Repeater {
            model: tasksModel

            delegate: Item {
                id: task
                required property int index
                required property var model

                readonly property real zoom: root.zoomAt(index * root.slot + root.slot / 2)
                readonly property bool running: model.IsWindow === true
                readonly property bool active: model.IsActive === true
                readonly property bool attention: model.IsDemandingAttention === true
                readonly property bool starting: model.IsStartup === true

                width: root.restSize * zoom + root.gap
                height: root.height
                Behavior on width { NumberAnimation { duration: 90; easing.type: Easing.OutQuad } }

                Kirigami.Icon {
                    id: icon
                    source: task.model.decoration
                    width: root.restSize * task.zoom
                    height: width
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: root.dotSpace + bounce.offset
                    active: mouse.containsMouse
                    Behavior on width { NumberAnimation { duration: 90; easing.type: Easing.OutQuad } }
                }

                // Starting apps bounce, as on macOS, until their window appears.
                QtObject { id: bounce; property real offset: 0 }
                SequentialAnimation {
                    running: task.starting
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) bounce.offset = 0
                    NumberAnimation { target: bounce; property: "offset"; to: root.restSize * 0.35; duration: 260; easing.type: Easing.OutQuad }
                    NumberAnimation { target: bounce; property: "offset"; to: 0; duration: 260; easing.type: Easing.InQuad }
                }

                // Running indicator: a dot; the active app gets a wider accent pill.
                Rectangle {
                    visible: task.running
                    width: task.active ? 12 : 5
                    height: 5
                    radius: 2.5
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: Math.max(1, Math.round((root.dotSpace - height) / 2))
                    color: task.attention ? "#f5a524"
                         : task.active ? Kirigami.Theme.highlightColor
                         : Kirigami.Theme.textColor
                    opacity: task.active || task.attention ? 1 : 0.55
                    Behavior on width { NumberAnimation { duration: 120 } }
                }

                PlasmaCore.ToolTipArea {
                    anchors.fill: parent
                    mainText: task.model.AppName || task.model.display || ""
                    subText: task.model.ChildCount > 1 ? i18np("%1 window", "%1 windows", task.model.ChildCount) : ""
                    location: Plasmoid.location
                }

                MouseArea {
                    id: mouse
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                    onClicked: mouse => {
                        const idx = tasksModel.makeModelIndex(task.index);
                        if (mouse.button === Qt.MiddleButton) {
                            tasksModel.requestNewInstance(idx);
                        } else if (mouse.button === Qt.RightButton) {
                            menu.openRelative();
                        } else if (task.running && task.active && task.model.ChildCount <= 1) {
                            tasksModel.requestToggleMinimized(idx);
                        } else {
                            tasksModel.requestActivate(idx);
                        }
                    }
                    onContainsMouseChanged: if (containsMouse) {
                        const g = task.mapToGlobal(0, 0);
                        tasksModel.requestPublishDelegateGeometry(tasksModel.makeModelIndex(task.index),
                            Qt.rect(g.x, g.y, task.width, task.height), task);
                    }
                }

                // A separate popup window, so the menu isn't clipped by the panel.
                PlasmaExtras.Menu {
                    id: menu
                    visualParent: task
                    placement: PlasmaExtras.Menu.TopPosedLeftAlignedPopup
                    PlasmaExtras.MenuItem {
                        text: i18n("Open new window")
                        icon: "window-new"
                        visible: task.model.CanLaunchNewInstance !== false
                        onClicked: tasksModel.requestNewInstance(tasksModel.makeModelIndex(task.index))
                    }
                    PlasmaExtras.MenuItem {
                        readonly property bool pinned: task.model.HasLauncher === true || task.model.IsLauncher === true
                        text: pinned ? i18n("Remove from dock") : i18n("Keep in dock")
                        icon: pinned ? "window-unpin" : "window-pin"
                        onClicked: {
                            const url = task.model.LauncherUrlWithoutIcon;
                            if (pinned) tasksModel.requestRemoveLauncher(url);
                            else tasksModel.requestAddLauncher(url);
                        }
                    }
                    PlasmaExtras.MenuItem {
                        text: task.model.ChildCount > 1 ? i18n("Close all windows") : i18n("Close")
                        icon: "window-close"
                        visible: task.running
                        onClicked: tasksModel.requestClose(tasksModel.makeModelIndex(task.index))
                    }
                    PlasmaExtras.MenuItem { separator: true }
                    PlasmaExtras.MenuItem {
                        text: i18n("Hide dock automatically")
                        checkable: true
                        checked: Plasmoid.configuration.autoHide
                        onClicked: root.setAutoHide(!Plasmoid.configuration.autoHide)
                    }
                }
            }
        }
    }
}
