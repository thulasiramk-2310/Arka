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
        warnings = [l for l in done.stderr.splitlines() if "qml" in l.lower() or "warning" in l.lower()]
        self.assertEqual(warnings, [], "the window printed warnings")
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
        """The wheel's "Hide private text": the window asks for suggestions at once and adds them as boxes."""
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

    def test_crop_to_a_square(self):
        from PIL import Image
        src = self.picture()
        # The "1:1" shape button sits second in the row of shapes, then Enter.
        self.run_window("crop", src, "p 112 590;r 112 590;k enter")
        out = self.wait_for("(cropped)")[0]
        with Image.open(out) as im:
            self.assertEqual(im.size, (500, 500))

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


if __name__ == "__main__":
    unittest.main()
