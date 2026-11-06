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
        function sizeText(urls) { return urls.length ? "1.2 MiB" : "" }
        function totalBytes(urls) { return String(urls[0]).indexOf("big") >= 0 ? 30000000 : 1200000 }
        function clearBlur() {}
        function setBlur(cx, cy, outer, inner) {}
        function formatsFor(urls) {
            if (!urls.length) return []
            if (urls.length > 1 && String(urls[0]).endsWith(".txt")) return ["zip-all", "zip"]
            if (String(urls[0]).endsWith(".jpg")) return ["webp", "smaller-image", "zip"]
            if (/\.(heic|xyz)$/i.test(String(urls[0]))) return []
            if (String(urls[0]).endsWith(".mp3")) return ["wav", "zip"]
            if (String(urls[0]).endsWith(".aac")) return ["mp3", "wav", "flac", "ogg", "m4a", "opus", "zip"]
            return ["webp", "pdf"]
        }
        function toolsFor(urls) {
            if (!urls.length) return []
            if (String(urls[0]).endsWith(".mp4")) return ["trim", "smaller", "bleep"]
            return ["crop", "redact"]
        }
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

    function test_zip_together_comes_first_for_several_files_and_zip_each_is_named_plainly() {
        const two = ["file:///tmp/a.txt", "file:///tmp/b.txt"]
        picker.open(two, false)
        compare(picker.formats[0], "zip-all")
        compare(picker.label("zip-all"), "Zip together")
        compare(picker.label("zip"), "ZIP each")
        // one file keeps the plain name
        picker.open(["file:///tmp/example.png"], false)
        compare(picker.label("zip"), "ZIP")
    }

    function test_picking_zip_together_sends_every_file_in_one_request() {
        const two = ["file:///tmp/a.txt", "file:///tmp/b.txt"]
        picker.open(two, false)
        picker.pick(0, picker.files)
        compare(backend.calls, 1)
        compare(backend.operation, "convert")
        compare(backend.chosen, "zip-all")
        compare(backend.chosenUrls.length, 2)
    }

    function test_arrow_tab_home_and_end_move_round_the_ring() {
        // example.png offers two slots in this test's backend
        compare(picker.formats.length, 2)
        compare(picker.hover, -1)
        picker.stepBy(1); compare(picker.hover, 0)
        picker.stepBy(1); compare(picker.hover, 1)
        picker.stepBy(1); compare(picker.hover, 0)      // wraps round
        picker.stepBy(-1); compare(picker.hover, 1)     // and back
        picker.jumpTo(false); compare(picker.hover, 0)
        picker.jumpTo(true); compare(picker.hover, 1)
    }

    function test_the_first_backward_step_starts_on_the_last_slot() {
        picker.stepBy(-1)
        compare(picker.hover, picker.formats.length - 1)
    }

    function test_keyboard_pick_launches_the_slot_you_are_on_once() {
        picker.stepBy(1)
        picker.pick(picker.hover, picker.files)
        compare(backend.calls, 1)
        compare(backend.chosen, "webp")
    }

    function test_make_smaller_asks_how_much_in_a_second_ring() {
        picker.open(["file:///tmp/photo.jpg"], false)
        compare(picker.formats[1], "smaller-image")
        picker.pick(1, picker.files)
        compare(backend.calls, 0)                       // nothing runs yet
        compare(picker.levelMode, true)
        compare(picker.formats.length, 3)
        compare(picker.label(picker.formats[0]), "Light")
        compare(picker.label(picker.formats[2]), "Strong")
        picker.pick(1, picker.files)
        compare(backend.calls, 1)
        compare(backend.operation, "convert")
        compare(backend.chosen, "smaller:medium")
        compare(backend.chosenUrls[0], "file:///tmp/photo.jpg")
    }

    function test_opening_the_wheel_again_leaves_the_level_ring() {
        picker.open(["file:///tmp/photo.jpg"], false)
        picker.pick(1, picker.files)
        compare(picker.levelMode, true)
        picker.open(["file:///tmp/photo.jpg"], false)
        compare(picker.levelMode, false)
        compare(picker.formats[1], "smaller-image")
    }

    function test_empty_ring_hint_explains_heic_and_stays_plain_otherwise() {
        picker.open(["file:///tmp/IMG_0001.HEIC"], false)
        compare(picker.formats.length, 0)
        verify(picker.emptyHint().indexOf("HEIC") >= 0)
        picker.open(["file:///tmp/notes.xyz"], false)
        compare(picker.emptyHint(), "This file can't be converted yet")
        picker.open(["file:///tmp/IMG_0001.heic"], true)
        compare(picker.emptyHint(), "Edits work on pictures, PDFs, sound and video")
    }

    // "Smaller" on the edit wheel asks how much first, then converts (it is not an edit window).
    function test_edit_smaller_asks_the_level_then_converts() {
        picker.open(["file:///tmp/clip.mp4"], true)
        picker.pick(1, picker.files)
        compare(backend.calls, 0)
        compare(picker.levelMode, true)
        compare(picker.formats[0], "smaller:light")
        picker.pick(2, picker.files)
        compare(backend.calls, 1)
        compare(backend.operation, "convert")
        compare(backend.chosen, "smaller:strong")
    }
    function test_other_edit_tools_still_run_as_edits() {
        picker.open(["file:///tmp/clip.mp4"], true)
        picker.pick(2, picker.files)
        compare(backend.operation, "edit")
        compare(backend.chosen, "bleep")
    }
    function test_size_is_shown_for_the_files() {
        picker.open(["file:///tmp/clip.mp4"], true)
        compare(picker.sizeLabel, "1.2 MiB")
    }

    function test_size_targets_are_offered_only_when_the_file_is_bigger() {
        picker.open(["file:///tmp/clip.mp4"], true)
        picker.pick(1, picker.files)
        compare(picker.formats.length, 3)
        picker.open(["file:///tmp/big-clip.mp4"], true)
        picker.pick(1, picker.files)
        compare(picker.formats.indexOf("smaller:10mb") >= 0, true)
        compare(picker.label("smaller:25mb"), "Under 25 MB")
    }
}
