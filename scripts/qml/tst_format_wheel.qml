import QtQuick
import QtTest
import "../../convert/wheel" as FormatWheel

TestCase {
    name: "FormatWheel"
    when: windowShown
    width: 800
    height: 600

    QtObject {
        id: backend
        signal opened(var urls, bool editMode)
        property int calls: 0
        property string operation: ""
        property string chosen: ""
        property var chosenUrls: []
        function dark() { return true }
        function clearBlur() {}
        function setBlur(cx, cy, outer, inner) {}
        function formatsFor(urls) {
            if (!urls.length) return []
            if (String(urls[0]).endsWith(".mp3")) return ["wav", "zip"]
            if (String(urls[0]).endsWith(".aac")) return ["mp3", "wav", "flac", "ogg", "m4a", "opus", "zip"]
            return ["webp", "pdf"]
        }
        function toolsFor(urls) { return urls.length ? ["crop", "redact"] : [] }
        function convert(format, urls) { calls++; operation = "convert"; chosen = format; chosenUrls = urls }
        function runTool(tool, urls) { calls++; operation = "edit"; chosen = tool; chosenUrls = urls }
    }

    FormatWheel.Wheel {
        id: picker
        // The production backend is a context property; use a fake in this instance.
        property var wheel: backend
        width: 800
        height: 600
    }

    function init() {
        backend.calls = 0
        backend.operation = ""
        backend.chosen = ""
        backend.chosenUrls = []
        picker.open(["file:///tmp/example.png"], false)
    }
    function cleanup() { picker.visible = false }

    function drop(urls, raw) {
        return {
            x: picker.cx, y: picker.cy - (picker.innerR + picker.outerR) / 2,
            urls: urls, accepted: false,
            getDataAsString: function (mime) { return raw || "" },
            accept: function (action) { this.accepted = true }
        }
    }

    function test_double_pick_launches_once() {
        picker.pick(0, picker.files)
        picker.pick(0, picker.files)
        compare(backend.calls, 1)
    }
    function test_raw_uri_list_is_used_for_the_actual_conversion() {
        const event = drop([], "# comment\r\nfile:///tmp/example.png\r\n")
        picker.handleDrop(event)
        compare(backend.calls, 1)
        compare(backend.chosenUrls[0], "file:///tmp/example.png")
        verify(event.accepted)
    }
    function test_late_uri_delivery_waits_for_a_choice() {
        picker.open([], false)
        picker.handleDrop(drop([], "file:///tmp/late.png\n"))
        compare(backend.calls, 0)
        compare(picker.clickMode, true)
        compare(picker.files[0], "file:///tmp/late.png")
        picker.pick(0, picker.files)
        compare(backend.calls, 1)
    }
    function test_changed_drop_refreshes_incompatible_formats() {
        picker.handleDrop(drop(["file:///tmp/music.mp3"], ""))
        compare(backend.calls, 0)
        compare(picker.formats[0], "wav")
        picker.pick(0, picker.files)
        compare(backend.chosen, "wav")
        compare(backend.chosenUrls[0], "file:///tmp/music.mp3")
    }
    function test_invalid_selection_and_empty_drop_do_not_launch() {
        picker.pick(-1, picker.files)
        picker.pick(99, picker.files)
        picker.pick(0, [])
        const event = drop([], "")
        picker.handleDrop(event)
        compare(backend.calls, 0)
        verify(!event.accepted)
    }
    function test_off_wheel_drop_cancels_instead_of_converting() {
        const event = drop(["file:///tmp/example.png"], "")
        event.y = picker.cy - picker.outerR - 20
        compare(picker.indexAt(event.x, event.y), -1)
        picker.handleDrop(event)
        compare(backend.calls, 0)
        verify(!event.accepted)
    }
    function test_edit_drop_uses_the_same_uri_fallback() {
        picker.open(["file:///tmp/example.png"], true)
        picker.handleDrop(drop([], "file:///tmp/example.png\n"))
        compare(backend.calls, 1)
        compare(backend.operation, "edit")
        compare(backend.chosen, "crop")
    }
    function test_more_does_not_launch_until_a_format_is_chosen() {
        picker.open(["file:///tmp/music.aac"], false)
        compare(picker.formats[5], "more")
        picker.pick(5, picker.files)
        compare(backend.calls, 0)
        compare(picker.formats[0], "opus")
        picker.pick(0, picker.files)
        compare(backend.calls, 1)
        compare(backend.chosen, "opus")
    }
    function test_finished_drag_does_not_close_more_choices() {
        picker.open(["file:///tmp/music.aac"], false)
        picker.pick(5, picker.files)
        picker.handleDragExit()
        wait(450)
        verify(picker.visible)
        verify(!picker.closing)
        compare(backend.calls, 0)
    }
    function test_literal_percent_in_filename_does_not_break_label() {
        picker.files = ["file:///tmp/100%.png"]
        compare(picker.fileLabel(), "100%.png")
        picker.files = ["file:///tmp/my%20file.png"]
        compare(picker.fileLabel(), "my file.png")
    }
}
