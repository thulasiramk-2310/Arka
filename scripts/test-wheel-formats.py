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
                       ("office", "office.contains(ext)"), ("text", "text.contains(ext)"),
                       ("other", "QFileInfo(path).isFile()")):
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
    def test_archives_offer_extract_and_arka_convert_accepts_it(self):
        suffixes = re.findall(r'"([^"]+)"', re.search(r"static const QStringList suffixes\{([^}]*)\}", SOURCE).group(1))
        for s in suffixes:
            with self.subTest(suffix=s):
                self.assertIn("extract", targets(f"archive{s}"))
        self.assertIn('return {"extract"};', SOURCE)

    def test_every_wheel_slot_is_accepted_by_arka_convert(self):
        exts = {"image": cpp_list("image"), "audio": cpp_list("audio"), "video": cpp_list("video"), "pdf": ["pdf"],
                "office": cpp_list("office"), "text": cpp_list("text"), "other": ["dat"]}
        for kind, slots in wheel_slots().items():
            for ext in exts[kind]:
                accepted = targets(f"file.{ext}")
                same = {"jpeg": "jpg", "tif": "tiff"}.get(ext, ext)
                for slot in slots:
                    if slot == same:
                        continue  # the wheel leaves out the format a file already is
                    if slot == "smaller-image" and ext not in wheel_smaller_images():
                        continue  # the wheel drops it for pictures that can't be made smaller
                    with self.subTest(kind=kind, ext=ext, slot=slot):
                        self.assertIn(slot, accepted)


class ZipTogetherTest(unittest.TestCase):
    def test_wheel_offers_zip_together_first_for_two_or_more_items(self):
        m = re.search(r"urls\.size\(\) >= 2\)(.*?)return common;", SOURCE, re.S)
        self.assertIsNotNone(m, "formatsFor() no longer has the two-or-more-items rule")
        self.assertIn('common.prepend(QStringLiteral("zip-all"))', m.group(1))

    def test_converter_runs_the_command_the_wheel_sends(self):
        text = CONVERT.read_text()
        self.assertIn('"zip-all"', text)
        self.assertIn("def zip_all(", text)
        # a request with fewer than two items is refused with the usage line, not run
        r = subprocess.run([sys.executable, "-B", str(CONVERT), "zip-all", "/nonexistent"],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 2)


class SmallerPictureRuleTest(unittest.TestCase):
    def test_wheel_and_converter_agree_on_which_pictures_can_be_made_smaller(self):
        m = re.search(r"SMALLER_IMAGE = \{([^}]*)\}", CONVERT.read_text())
        converter = set(re.findall(r'"([^"]+)"', m.group(1)))
        self.assertEqual(wheel_smaller_images(), converter)


class EditToolsTest(unittest.TestCase):
    """Every edit the edit wheel can show has a name on the ring and something that runs it."""

    def tools(self):
        body = re.search(r"static QStringList toolsForFile\(.*?\n}\n", SOURCE, re.S).group(0)
        lists = re.findall(r"out = \{([^}]*)\}", body)
        found = {t for l in lists for t in re.findall(r'"([^"]+)"', l)}
        found |= set(re.findall(r'multi << QStringLiteral\("([^"]+)"\)', re.search(r"QStringList toolsFor\(.*?\n    }\n", SOURCE, re.S).group(0)))
        return found

    def test_every_tool_has_a_label(self):
        qml = (ROOT / "convert" / "wheel" / "Wheel.qml").read_text()
        names = re.search(r"readonly property var names: \(\{(.*?)\}\)", qml, re.S).group(1)
        for tool in self.tools():
            with self.subTest(tool=tool):
                self.assertRegex(names, r'(^|[\s,{])"?%s"?:' % re.escape(tool))

    def test_every_tool_is_run_by_something(self):
        run = re.search(r"Q_INVOKABLE void runTool\(.*?\n    }\n", SOURCE, re.S).group(0)
        windowed = set(re.findall(r'"([^"]+)"', re.search(r"windowed\{([^}]*)\}", run).group(1)))
        handled = windowed | set(re.findall(r'tool == QLatin1String\("([^"]+)"\)', run))
        qml = (ROOT / "convert" / "wheel" / "Wheel.qml").read_text()
        for tool in self.tools():
            with self.subTest(tool=tool):
                # "smaller" opens the level ring in the QML, which then converts.
                self.assertTrue(tool in handled or (tool in ("smaller", "resize") and f'formats[i] === "{tool}"' in qml), tool)

    def test_edit_window_knows_every_windowed_tool(self):
        run = re.search(r"Q_INVOKABLE void runTool\(.*?\n    }\n", SOURCE, re.S).group(0)
        windowed = set(re.findall(r'"([^"]+)"', re.search(r"windowed\{([^}]*)\}", run).group(1))) | {"split"}
        edit = (ROOT / "convert" / "edit" / "main.cpp").read_text()
        accepted = set(re.findall(r'"([^"]+)"', re.search(r"static const QStringList tools\{([^}]*)\}", edit).group(1)))
        self.assertLessEqual(windowed, accepted)


if __name__ == "__main__":
    unittest.main()
