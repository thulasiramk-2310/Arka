#!/usr/bin/env python3
"""Check that every format the format wheel offers is one arka-convert accepts.

The wheel (convert/wheel/main.cpp) keeps its own short list per kind of file so
it can draw instantly; this reads that list and asks arka-convert about the same
files. Nothing is converted. Run: python3 scripts/test-wheel-formats.py
"""

from pathlib import Path
import re
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
SOURCE = (ROOT / "convert" / "wheel" / "main.cpp").read_text()
CONVERT = ROOT / "convert" / "arka-convert"


def cpp_list(name):
    m = re.search(r"static const QStringList %s\{([^}]*)\}" % name, SOURCE)
    return re.findall(r'"([^"]+)"', m.group(1))


def wheel_slots():
    """kind -> the slots formatsForFile() returns, in order."""
    slots = {}
    for kind, cond in (("image", "image.contains(ext)"), ("audio", "audio.contains(ext)"),
                       ("video", "video.contains(ext)"), ("pdf", 'ext == "pdf"'),
                       ("office", "office.contains(ext)"), ("other", "QFileInfo(path).isFile()")):
        m = re.search(re.escape(cond) + r"\)\s*out = \{([^}]*)\};", SOURCE)
        slots[kind] = re.findall(r'"([^"]+)"', m.group(1))
    return slots


def wheel_smaller_images():
    """The pictures the wheel offers "smaller picture" for: the ones formatsForFile() does not strip it from."""
    m = re.search(r'if \(ext != "png"(.*?)\)\s*out\.removeAll\(QStringLiteral\("smaller-image"\)\)', SOURCE, re.S)
    return {"png"} | set(re.findall(r'ext != "([^"]+)"', 'ext != "png"' + m.group(1)))


def targets(filename):
    out = subprocess.run([sys.executable, "-B", str(CONVERT), "--targets", filename],
                         capture_output=True, text=True, check=True).stdout
    return out.split()


class WheelFormatsTest(unittest.TestCase):
    def test_every_wheel_slot_is_accepted_by_arka_convert(self):
        exts = {"image": cpp_list("image"), "audio": cpp_list("audio"), "video": cpp_list("video"), "pdf": ["pdf"],
                "office": cpp_list("office"), "other": ["txt"]}
        for kind, slots in wheel_slots().items():
            for ext in exts[kind]:
                accepted = targets(f"file.{ext}")
                same = "jpg" if ext == "jpeg" else ext
                for slot in slots:
                    if slot == same:
                        continue  # the wheel leaves out the format a file already is
                    if slot == "smaller-image" and ext not in wheel_smaller_images():
                        continue  # the wheel drops it for pictures that can't be made smaller
                    with self.subTest(kind=kind, ext=ext, slot=slot):
                        self.assertIn(slot, accepted)


class SmallerPictureRuleTest(unittest.TestCase):
    def test_wheel_and_converter_agree_on_which_pictures_can_be_made_smaller(self):
        m = re.search(r"SMALLER_IMAGE = \{([^}]*)\}", CONVERT.read_text())
        converter = set(re.findall(r'"([^"]+)"', m.group(1)))
        self.assertEqual(wheel_smaller_images(), converter)


if __name__ == "__main__":
    unittest.main()
