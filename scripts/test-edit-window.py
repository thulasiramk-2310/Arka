#!/usr/bin/env python3
"""Drive the real arka-edit window (no screen needed) and check what it saves.

Each test opens the window offscreen, replays pointer and key events through its
test hook (ARKA_EDIT_EVENTS), saves with Enter, and checks the file arka-convert
makes next to the original. The window must not print any QML warning.

Needs a built arka-edit: set ARKA_EDIT to its path (CMake builds it in
convert/edit). Without one, the tests are skipped. Needs ffmpeg and Pillow, like
scripts/test-arka-convert.py. Run: ARKA_EDIT=/path/to/arka-edit python3 scripts/test-edit-window.py
"""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent
EDIT = os.environ.get("ARKA_EDIT", "")
CONVERT = ROOT / "convert" / "arka-convert"
PLAYER = ROOT / "convert" / "edit" / "Player.qml"
# The only line a window may print: Qt Multimedia names its FFmpeg when a sound or video tool loads
# the player. Anything else on stderr (a QML warning, an audio-device error) fails the test.
ALLOWED = ("qt.multimedia.ffmpeg: Using Qt multimedia with FFmpeg",)


def ready():
    return EDIT and Path(EDIT).is_file() and os.access(EDIT, os.X_OK)


@unittest.skipUnless(ready(), "set ARKA_EDIT to a built arka-edit to run these")
class EditWindowTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.bin = self.dir / "bin"
        self.bin.mkdir()
        (self.bin / "arka-convert").symlink_to(CONVERT)
        (self.dir / "config").mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def run_window(self, tool, path, events, extra=()):
        env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}", QT_QPA_PLATFORM="offscreen",
                   ARKA_CONVERT_QUIET="1", XDG_CONFIG_HOME=str(self.dir / "config"),
                   ARKA_EDIT_EVENTS=events, XDG_RUNTIME_DIR=str(self.dir), QT_FORCE_STDERR_LOGGING="1",
                   # The screenshot hook also closes a window that is left with nothing to save.
                   ARKA_EDIT_SCREENSHOT=str(self.dir / "config" / "last.png"))
        done = subprocess.run([EDIT, tool, str(path), *extra], env=env, capture_output=True, text=True, timeout=120)
        noise = [l for l in done.stderr.splitlines() if l.strip() and not l.startswith(ALLOWED)]
        self.assertEqual(noise, [], "the window printed to stderr")
        return done

    def wait_for(self, name, seconds=60):
        """arka-convert runs on after the window closes; wait for its file."""
        end = time.time() + seconds
        while time.time() < end:
            found = [p for p in self.dir.iterdir() if name in p.name]
            if found and all(p.stat().st_size > 0 for p in found):
                time.sleep(0.5)
                return found
            time.sleep(0.3)
        self.fail(f"no file with '{name}' was saved; have {sorted(p.name for p in self.dir.iterdir())}")

    def crop_geometry(self):
        """Where the crop box is in the last screenshot: its accent-coloured frame, left of the side panel."""
        from PIL import Image
        with Image.open(self.dir / "config" / "last.png") as im:
            rgb = im.convert("RGB")
            w, h = rgb.size
            xs, ys = [], []
            for y in range(80, h - 160):
                for x in range(0, w - 290):
                    r, g, b = rgb.getpixel((x, y))
                    if abs(r - 0x4D) < 12 and abs(g - 0xB4) < 12 and abs(b - 0x91) < 12:
                        xs.append(x); ys.append(y)
        self.assertTrue(xs, "the crop box was not found on screen")
        left, right, top, bottom = min(xs), max(xs), min(ys), max(ys)
        return {"right": right - 3, "middle": (top + bottom) // 2, "half": (right - left) // 2}

    def picture(self):
        from PIL import Image
        path = self.dir / "photo.png"
        Image.new("RGB", (800, 500), (255, 255, 255)).save(path)
        return path

    def sound(self):
        path = self.dir / "talk.mp3"
        subprocess.run(["ffmpeg", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=300:duration=6", str(path)],
                       check=True)
        return path

    def test_annotate_draws_and_saves_with_enter(self):
        from PIL import Image
        src = self.picture()
        # A pen stroke across the middle of the picture, then Enter.
        self.run_window("annotate", src, "p 150 300;m 250 320;m 350 300;m 450 320;r 450 320;k enter")
        out = self.wait_for("(annotated)")[0]
        with Image.open(out) as im:
            px = list(im.convert("RGB").get_flattened_data())
            self.assertTrue(any(p != (255, 255, 255) for p in px), "nothing was drawn")

    def test_black_out_box_is_black_in_the_copy(self):
        from PIL import Image
        src = self.picture()
        self.run_window("redact", src, "p 200 250;m 300 300;m 360 340;r 360 340;k enter")
        out = self.wait_for("(redacted)")[0]
        with Image.open(out) as im:
            colors = set(im.convert("RGB").get_flattened_data())
            self.assertIn((0, 0, 0), colors)
            self.assertIn((255, 255, 255), colors)          # only the box, not the whole picture

    def test_nothing_is_saved_until_there_is_something_to_save(self):
        src = self.picture()
        self.run_window("annotate", src, "k enter")
        time.sleep(3)
        self.assertEqual(sorted(p.name for p in self.dir.iterdir() if p.is_file()), ["photo.png"])

    def test_split_cuts_at_the_middle_by_default(self):
        src = self.sound()
        self.run_window("split", src, "k enter")
        parts = self.wait_for("(part ")
        self.assertEqual(len(parts), 2)

    def test_bleep_part_dragged_on_the_timeline_is_saved(self):
        src = self.sound()
        # Drag across the left part of the timeline, then Enter.
        self.run_window("bleep", src, "p 200 250;m 260 250;m 320 250;r 320 250;k enter")
        self.wait_for("(bleeped)")

    def test_hide_private_text_starts_with_the_suggested_boxes(self):
        """The wheel's "Find private text": the window asks for suggestions at once and adds them as boxes."""
        from PIL import Image
        src = self.picture()
        fake = self.bin / "arka-convert"
        fake.unlink()
        fake.write_text("#!/bin/sh\n"
                        "if [ \"$1\" = --find-private ]; then echo '[{\"kind\": \"email\", \"box\": [40, 30, 200, 60]}]'; exit 0; fi\n"
                        f"exec {CONVERT} \"$@\"\n")
        fake.chmod(0o755)
        self.run_window("redact", src, "k enter", extra=("--find-private",))
        out = self.wait_for("(redacted)")[0]
        with Image.open(out) as im:
            rgb = im.convert("RGB")
            self.assertEqual(rgb.getpixel((100, 50)), (0, 0, 0))          # inside the suggested box
            self.assertEqual(rgb.getpixel((500, 300)), (255, 255, 255))   # elsewhere untouched

    def test_zoom_in_makes_the_same_drag_cover_less(self):
        """After one zoom in (x1.5), a drag across the same screen area covers 1/1.5 as much in each direction."""
        from PIL import Image
        src = self.picture()
        drag = "p 330 280;m 360 300;m 410 340;r 410 340"
        self.run_window("redact", src, drag + ";k enter")
        plain = self.wait_for("(redacted)")[0]
        zoomed_src = self.dir / "zoomed.png"
        shutil.copy(src, zoomed_src)
        # The "+" button sits on the title's line, at the right of the picture's area.
        self.run_window("redact", zoomed_src, "p 742 38;r 742 38;" + drag + ";k enter")
        zoomed = self.wait_for("zoomed (redacted)")[0]

        def black(path):
            with Image.open(path) as im:
                return sum(1 for px in im.convert("L").get_flattened_data() if px < 20)
        ratio = black(plain) / max(1, black(zoomed))
        self.assertTrue(1.8 < ratio < 2.8, f"area ratio {ratio:.2f}, expected about 2.25")

    def test_crop_to_a_square(self):
        from PIL import Image
        src = self.picture()
        # The "1:1" shape button sits second in the row of shapes, then Enter.
        done = self.run_window("crop", src, "p 112 590;r 112 590;k enter")
        self.assertNotIn("multimedia", done.stderr)                  # pictures never load the player
        out = self.wait_for("(cropped)")[0]
        with Image.open(out) as im:
            self.assertEqual(im.size, (500, 500))

    def test_crop_by_dragging_a_side(self):
        from PIL import Image
        src = self.picture()
        # Free shape: the right side's handle, dragged left by half the picture's width on screen.
        self.run_window("crop", src, "m 1 1")                       # a first look, to find the box on screen
        geo = self.crop_geometry()
        x, y = geo["right"], geo["middle"]
        self.run_window("crop", src, f"p {x} {y};m {x - 60} {y};m {x - geo['half']} {y};r {x - geo['half']} {y};k enter")
        out = self.wait_for("(cropped)")[0]
        with Image.open(out) as im:
            self.assertEqual(im.size[1], 500)                         # as tall as before
            self.assertTrue(350 <= im.size[0] <= 450, im.size)          # about half as wide

    def test_background_adds_room_around_the_picture(self):
        from PIL import Image
        src = self.picture()
        self.run_window("background", src, "k enter")
        out = self.wait_for("(with background)")[0]
        with Image.open(out) as im:
            self.assertEqual(im.size, (800 + 120, 500 + 120))           # 60 px on every side by default

    def test_trim_saves_the_part_kept(self):
        src = self.sound()
        self.run_window("trim", src, "k enter")
        self.wait_for("(trimmed)")

    def test_a_missing_file_is_refused_plainly(self):
        env = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_FORCE_STDERR_LOGGING="1")
        done = subprocess.run([EDIT, "crop", str(self.dir / "nothing.png")], env=env, capture_output=True,
                              text=True, timeout=30)
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage", done.stderr)


def qmltestrunner():
    """Qt 6's qmltestrunner. A plain `qmltestrunner` on PATH can be Qt 5's, which cannot read these tests."""
    for d in ("/usr/lib64/qt6/bin", "/usr/lib/qt6/bin"):
        if os.access(f"{d}/qmltestrunner", os.X_OK):
            return f"{d}/qmltestrunner"
    return shutil.which("qmltestrunner6") or shutil.which("qmltestrunner-qt6")


@unittest.skipUnless(qmltestrunner() and shutil.which("ffmpeg"), "needs qmltestrunner (Qt 6) and ffmpeg")
class PlayerTest(unittest.TestCase):
    """Player.qml on its own: plays a part and stops at its end. No sound device is needed: without
    one, Qt Multimedia plays against a clock."""

    def test_sound(self):
        self.play(["-f", "lavfi", "-i", "sine=frequency=300:duration=6"], "talk.wav")

    def test_video(self):
        # mpeg4 and aac are encoders that Fedora's ffmpeg-free has too.
        self.play(["-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=6", "-f", "lavfi", "-i",
                   "sine=frequency=300:duration=6", "-c:v", "mpeg4", "-c:a", "aac", "-shortest"], "clip.mp4")

    def play(self, make, name):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            sound = tmp / name
            subprocess.run(["ffmpeg", "-loglevel", "error", *make, str(sound)], check=True)
            (tmp / "Player.qml").write_text(PLAYER.read_text())
            (tmp / "tst_player.qml").write_text(PLAYER_TEST.replace("SOUND", sound.as_uri()))
            env = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_FORCE_STDERR_LOGGING="1",
                       XDG_RUNTIME_DIR=str(tmp), PULSE_SERVER="unix:/nonexistent", PIPEWIRE_REMOTE="/nonexistent")
            done = subprocess.run([qmltestrunner(), "-input", str(tmp / "tst_player.qml")], env=env,
                                  capture_output=True, text=True, timeout=120)
            self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
            self.assertIn(" 0 failed", done.stdout)
            self.assertEqual(done.stdout.count("PASS   : qmltestrunner::Player::test_"), 4, done.stdout)
            self.assertNotIn("qml:", done.stderr)


PLAYER_TEST = """
import QtQuick
import QtTest

TestCase {
    name: "Player"
    width: 200; height: 100
    Player { id: p; source: "SOUND" }

    function cleanup() { if (p.playing) p.toggle(0, 0); tryVerify(() => !p.playing, 3000) }

    function test_a_the_file_is_opened_only_on_play() {
        verify(!p.opened)
    }
    function test_b_a_part_plays_and_stops_at_its_end() {
        p.toggle(1, 2)
        tryVerify(() => p.playing, 3000)
        tryVerify(() => !p.playing, 5000)
        verify(p.position >= 1.9 && p.position < 2.6, "stopped at " + p.position)
        compare(p.error, "")
    }
    function test_c_the_old_position_does_not_stop_the_next_part() {
        p.toggle(3, 4)                               // ends at 4 s: later than the next part's end
        tryVerify(() => !p.playing && p.position >= 3.9, 5000)
        p.toggle(1, 2)
        tryVerify(() => p.playing && p.position > 1.2 && p.position < 2, 3000)
        tryVerify(() => !p.playing, 5000)
        verify(p.position >= 1.9 && p.position < 2.6, "stopped at " + p.position)
    }
    function test_d_a_part_with_no_length_plays_nothing() {
        p.toggle(2, 2.01)
        wait(300)
        verify(!p.playing)
    }
}
"""


if __name__ == "__main__":
    unittest.main()
