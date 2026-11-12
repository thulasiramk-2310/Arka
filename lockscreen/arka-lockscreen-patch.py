#!/usr/bin/python3
"""Restyle Plasma's session lock screen (Meta+L) to the ArkaOS design, at image build time.

While idle the clock is large, light and to the left, as on the ArkaOS login screen; when the password
field appears it moves back above the account, where KDE puts it. Only the clock block and its shadow
change. If Plasma's file no longer contains the exact blocks below, this exits non-zero and the image
build stops, so a changed upstream file can never ship half-patched. kscreenlocker also falls back to
its built-in locker if the QML fails to load, so a mistake here cannot lock anyone out.
"""
import sys

PATH = "/usr/share/plasma/shells/org.kde.plasma.desktop/contents/lockscreen/LockScreenUi.qml"

EDITS = [
    ("""        DropShadow {
            id: clockShadow
            anchors.fill: clock
            source: clock
""", """        DropShadow {
            id: clockShadow
            anchors.fill: clock
            source: clock
            // ArkaOS: follow the clock's size change, or an unscaled copy shows underneath.
            scale: clock.scale
            transformOrigin: clock.transformOrigin
"""),
    ("""            visible: y > 0 && config.alwaysShowClock
            anchors.horizontalCenter: parent.horizontalCenter
            y: (mainBlock.userList.y + mainStack.y)/2 - height/2
""", """            visible: y > 0 && config.alwaysShowClock
            // ArkaOS: while idle the clock is large and to the left, as on the ArkaOS login screen; when the
            // password field appears it moves back above the account, where KDE puts it.
            x: lockScreenRoot.uiVisible ? (parent.width - width) / 2 : parent.width * 0.083
            y: lockScreenRoot.uiVisible ? (mainBlock.userList.y + mainStack.y)/2 - height/2 : parent.height * 0.27
            scale: lockScreenRoot.uiVisible ? 1 : 1.35
            transformOrigin: lockScreenRoot.uiVisible ? Item.Center : Item.TopLeft
            Behavior on x { NumberAnimation { duration: Kirigami.Units.longDuration; easing.type: Easing.OutCubic } }
            Behavior on y { NumberAnimation { duration: Kirigami.Units.longDuration; easing.type: Easing.OutCubic } }
            Behavior on scale { NumberAnimation { duration: Kirigami.Units.longDuration; easing.type: Easing.OutCubic } }
            // The time in a light weight, as on the login screen (KDE draws it DemiBold).
            Component.onCompleted: if (children.length > 0 && children[0].font) children[0].font.weight = Font.Light
"""),
]


def main(path=PATH):
    text = open(path, encoding="utf-8").read()
    for old, new in EDITS:
        if text.count(old) != 1:
            sys.exit(f"arka-lockscreen-patch: expected block not found exactly once in {path}; Plasma changed it")
        text = text.replace(old, new)
    open(path, "w", encoding="utf-8").write(text)
    print("arka-lockscreen-patch: applied")


if __name__ == "__main__":
    main(*sys.argv[1:])
