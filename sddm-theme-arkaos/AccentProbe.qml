import QtQuick
import org.kde.kirigami as Kirigami

// Reads the accent colour from the KDE colour scheme the login screen uses.
// "Apply Plasma Settings" in System Settings > Login Screen copies the
// user's scheme here, so the login screen follows the desktop accent.
// Loaded through a Loader: if Kirigami is missing, Main.qml falls back.
QtObject {
    readonly property color accent: Kirigami.Theme.highlightColor
    // When no KDE colour scheme is loaded, Kirigami reports Qt's built-in
    // palette (black text on #efefef). That accent means nothing, so say so.
    readonly property bool fromScheme:
        !(Qt.colorEqual(Kirigami.Theme.textColor, "#000000")
          && Qt.colorEqual(Kirigami.Theme.backgroundColor, "#efefef"))
}
