// Plays the file being edited, so a trim, a cut or a part to bleep can be heard (and, for video,
// seen) before anything is saved. Loaded by Edit.qml through a Loader, only for the sound and video
// tools: if Qt Multimedia is not installed this file fails to load, and the window simply has no
// Play button.
//
// Everything else the window shows (frame, waveform, length, details) comes from arka-convert, in its
// sandbox. Playing is the one exception: Qt Multimedia decodes the file in this process, as any media
// player on the desktop does. So the file is not opened here until Play is pressed.
import QtQuick
import QtMultimedia

Item {
    id: p
    property url source                          // the file; opened on the first play()
    property real stopAt: -1                     // seconds; -1 plays to the end
    property real startAt: 0
    property bool seeking: false                 // until the player reports a position in [startAt, stopAt)
    readonly property bool playing: player.playbackState === MediaPlayer.PlayingState
    readonly property real position: player.position / 1000
    readonly property bool hasVideo: player.hasVideo
    readonly property bool opened: player.source.toString() !== ""
    property string error: ""                    // why the last play failed, in the player's words

    MediaPlayer {
        id: player
        videoOutput: screen
        onPositionChanged: {
            const s = player.position / 1000
            // A seek is asynchronous: the old position can be reported once more after it (for
            // example the end of the last part played), and must not stop the new part at once.
            if (p.seeking) {
                if (s >= p.startAt - 0.25 && (p.stopAt < 0 || s < p.stopAt)) p.seeking = false
                return
            }
            if (p.stopAt >= 0 && s >= p.stopAt) player.pause()
        }
        onErrorOccurred: (code, text) => { p.error = text || "it could not be played"; p.seeking = false }
    }
    VideoOutput {
        id: screen
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectFit
    }
    // The audio device is made on the first play, not when the window opens.
    Component { id: speaker; AudioOutput {} }
    // If the player never reports a position in the part (a very short part), stop waiting for it.
    Timer { id: settle; interval: 1500; onTriggered: p.seeking = false }

    // Play from a to b (seconds; b = -1 plays to the end), or stop if already playing.
    function toggle(a, b) {
        if (playing) { player.pause(); return }
        a = Math.max(0, a)
        if (b >= 0 && b - a < 0.05) return       // nothing to play
        error = ""
        if (!player.audioOutput) player.audioOutput = speaker.createObject(p)
        if (player.source != source) player.source = source
        startAt = a
        stopAt = b
        seeking = true
        settle.restart()
        player.position = a * 1000
        player.play()
    }
}
