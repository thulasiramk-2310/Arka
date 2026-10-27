/*
    ArkaOS launcher: a popup from the top bar with pinned apps, categories,
    search and the session buttons. The app list, search, pinned list and
    session actions come from Plasma's own models (the ones Kickoff uses), so
    installed apps, menu edits and pins behave the same as in stock Plasma.

    SPDX-License-Identifier: GPL-3.0-or-later
*/

import QtQuick
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.private.kicker as Kicker

PlasmoidItem {
    id: root

    readonly property Kicker.RootModel rootModel: Kicker.RootModel {
        autoPopulate: false
        appletInterface: root
        appNameFormat: 0 // name only; the description is shown on its own line
        flat: true
        sorted: true
        showSeparators: false
        showRootSeparator: false
        showTopLevelItems: false
        showAllApps: true
        showAllAppsCategorized: false
        showRecentApps: false
        showRecentDocs: false
        showPowerSession: false
        showFavoritesPlaceholder: true // row 0 stands for the pinned apps, as in Kickoff

        Component.onCompleted: {
            favoritesModel.initForClient("org.arkaos.launcher.favorites.instance-" + Plasmoid.id)
            // Pins are shared by every launcher of the account, so an account
            // that already has pins keeps them. A new one gets the list in
            // kicker-extra-favoritesrc, which the image sets to ArkaOS's apps.
            if (!Plasmoid.configuration.favoritesPorted) {
                if (favoritesModel.count < 1) {
                    favoritesModel.portOldFavorites([])
                }
                Plasmoid.configuration.favoritesPorted = true
            }
        }
    }

    // Apps and settings only: no file, web or shell runners, so what the
    // search field says it searches is what it searches.
    readonly property Kicker.RunnerModel runnerModel: Kicker.RunnerModel {
        appletInterface: root
        runners: ["krunner_services", "krunner_systemsettings"]
        mergeResults: true
        favoritesModel: root.rootModel.favoritesModel
    }

    // Apps opened recently, from Plasma's own usage history, which stays on this
    // computer. It is only shown when the user turns it on.
    readonly property Kicker.RecentUsageModel recentModel: Kicker.RecentUsageModel {
        shownItems: Kicker.RecentUsageModel.OnlyApps
        ordering: Kicker.RecentUsageModel.Recent
    }

    Kicker.ProcessRunner {
        id: processRunner
    }

    Plasmoid.icon: "start-here-kde"
    toolTipMainText: Plasmoid.title
    toolTipSubText: ""

    preferredRepresentation: compactRepresentation
    fullRepresentation: FullRepresentation {
        focus: true
    }

    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: i18nc("@action:inmenu", "Edit Applications…")
            icon.name: "kmenuedit"
            visible: Plasmoid.immutability !== PlasmaCore.Types.SystemImmutable
            onTriggered: processRunner.runMenuEditor()
        }
    ]

    Component.onCompleted: {
        Plasmoid.activationTogglesExpanded = true
    }
}
