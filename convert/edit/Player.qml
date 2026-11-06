// Plays the file being edited, so a trim, a cut or a part to bleep can be heard (and, for video,
// seen) before anything is saved. Loaded by Edit.qml through a Loader: if Qt Multimedia is not
// installed this file fails to load, and the window simply has no Play button.
import QtQuick
import QtMultimedia

Item {
    id: p
    property url source
    property real stopAt: -1                     // seconds; -1 plays to the end
    readonly property bool playing: player.playbackState === MediaPlayer.PlayingState
    readonly property real position: player.position / 1000
    readonly property bool hasVideo: player.hasVideo

    MediaPlayer {
        id: player
        source: p.source
        audioOutput: AudioOutput {}
        videoOutput: screen
        onPositionChanged: if (p.stopAt >= 0 && player.position / 1000 >= p.stopAt) player.pause()
    }
    VideoOutput {
        id: screen
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectFit
    }

    // Play from a to b (seconds), or stop if already playing.
    function toggle(a, b) {
        if (playing) { player.pause(); return }
        stopAt = b
        player.position = Math.max(0, a) * 1000
        player.play()
    }
}
