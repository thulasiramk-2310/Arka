import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

Kirigami.FormLayout {
    property alias cfg_showTagline: show.checked
    property alias cfg_tagline: text.text
    property bool cfg_showTaglineDefault
    property string cfg_taglineDefault
    QQC2.CheckBox { id: show; Kirigami.FormData.label: i18n("Tagline:"); text: i18n("Show next to the name") }
    QQC2.TextField { id: text; Kirigami.FormData.label: i18n("Text:"); enabled: show.checked; maximumLength: 60 }
}
