import QtQuick
import org.kde.plasma.private.battery

// Battery state for the status corner, from the same model KDE's own login
// screen (Breeze) uses. Loaded through a Loader: if the Plasma battery module
// is missing, Main.qml simply shows no battery.
QtObject {
    property BatteryControlModel model: BatteryControlModel {}
    readonly property bool present: model.hasInternalBatteries
    readonly property int percent: model.percent
    readonly property bool charging: model.pluggedIn
}
