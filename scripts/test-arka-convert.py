#!/usr/bin/env python3
"""Exercise arka-convert with real conversions inside the real sandbox.

Needs bubblewrap, ffmpeg, poppler-utils and Pillow, the same tools ArkaOS ships.
Files are made in a temporary folder; nothing else on the system is touched.
Run: python3 scripts/test-arka-convert.py
"""

import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent.parent / "convert" / "arka-convert"
ENV = dict(os.environ, ARKA_CONVERT_QUIET="1")


def encoders():
    out = subprocess.run(["ffmpeg", "-hide_banner", "-encoders"], capture_output=True, text=True).stdout
    return out


def load_script():
    from importlib.machinery import SourceFileLoader
    from importlib.util import module_from_spec, spec_from_loader
    loader = SourceFileLoader("arka_convert", str(SCRIPT))
    mod = module_from_spec(spec_from_loader("arka_convert", loader))
    loader.exec_module(mod)
    return mod


def convert(*args, env=None):
    return subprocess.run([sys.executable, str(SCRIPT), *args], capture_output=True, text=True,
                          env=env or ENV)


def ffmpeg(*args):
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", *args], check=True)


class ArkaConvertTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def names(self):
        return sorted(p.name for p in self.dir.iterdir())

    def png(self, name="pic.png", mode="RGBA"):
        from PIL import Image
        path = self.dir / name
        Image.new(mode, (64, 48), (200, 30, 30, 0) if mode == "RGBA" else (200, 30, 30)).save(path)
        return path

    def test_targets_leave_out_the_source_format(self):
        pic = self.png()
        got = convert("--targets", str(pic)).stdout.split()
        self.assertEqual(got, ["jpg", "webp", "pdf"])
        self.assertEqual(convert("--targets", str(self.dir / "x.txt")).stdout.split(), [])

    def test_image_to_each_format_keeps_original(self):
        pic = self.png()
        before = pic.read_bytes()
        for fmt in ("jpg", "webp", "pdf"):
            r = convert(fmt, str(pic))
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertTrue((self.dir / f"pic.{fmt}").stat().st_size > 0)
        self.assertEqual(pic.read_bytes(), before)
        self.assertEqual(self.names(), ["pic.jpg", "pic.pdf", "pic.png", "pic.webp"])
        self.assertTrue((self.dir / "pic.pdf").read_bytes().startswith(b"%PDF"))

    def test_transparent_png_becomes_white_not_black_jpeg(self):
        from PIL import Image
        pic = self.png()
        convert("jpg", str(pic))
        with Image.open(self.dir / "pic.jpg") as im:
            r, g, b = im.getpixel((5, 5))
        self.assertTrue(r > 240 and g > 240 and b > 240, (r, g, b))

    def test_never_overwrites_a_file_with_the_same_name(self):
        pic = self.png()
        (self.dir / "pic.jpg").write_bytes(b"mine")
        convert("jpg", str(pic))
        convert("jpg", str(pic))
        self.assertEqual((self.dir / "pic.jpg").read_bytes(), b"mine")
        self.assertEqual(self.names(), ["pic (1).jpg", "pic (2).jpg", "pic.jpg", "pic.png"])

    def test_pdf_to_png_one_file_per_page(self):
        from PIL import Image
        a, b = Image.new("RGB", (80, 60), "white"), Image.new("RGB", (80, 60), "black")
        pdf = self.dir / "doc.pdf"
        a.save(pdf, "PDF", save_all=True, append_images=[b])
        r = convert("png", str(pdf))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.names(), ["doc-1.png", "doc-2.png", "doc.pdf"])

    def test_audio(self):
        wav = self.dir / "tone.wav"
        ffmpeg("-f", "lavfi", "-i", "sine=frequency=440:duration=1", str(wav))
        formats = ["flac", "opus", "ogg", "m4a", "mp3"]
        for fmt in formats:
            r = convert(fmt, str(wav))
            self.assertEqual(r.returncode, 0, f"{fmt}: {r.stderr}")
            self.assertTrue((self.dir / f"tone.{fmt}").stat().st_size > 0, fmt)
        self.assertEqual(convert("flac", str(self.dir / "tone.mp3")).returncode, 0)

    def make_video(self):
        mkv = self.dir / "clip.mkv"
        ffmpeg("-f", "lavfi", "-i", "testsrc=size=320x240:rate=10:duration=1",
               "-f", "lavfi", "-i", "sine=duration=1", "-shortest", str(mkv))
        return mkv

    def test_video_to_webm_mkv_and_audio_only(self):
        mkv = self.make_video()
        for fmt in ("webm", "mp3"):
            r = convert(fmt, str(mkv))
            self.assertEqual(r.returncode, 0, f"{fmt}: {r.stderr}")
        r = convert("mkv", str(self.dir / "clip.webm"))   # clip.mkv is taken: becomes "clip (1).mkv"
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue((self.dir / "clip (1).mkv").stat().st_size > 0)
        self.assertTrue((self.dir / "clip.mp3").stat().st_size > 0)

    @unittest.skipUnless("libopenh264" in encoders(), "this ffmpeg has no H.264 encoder (ArkaOS's does)")
    def test_video_to_mp4(self):
        mkv = self.make_video()
        r = convert("mp4", str(mkv))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue((self.dir / "clip.mp4").stat().st_size > 0)

    def test_scratch_folder_is_always_removed(self):
        pic = self.png()
        convert("jpg", str(pic))
        bad = self.dir / "broken.png"
        bad.write_bytes(b"not a picture")
        r = convert("jpg", str(bad))
        self.assertEqual(r.returncode, 1)
        self.assertFalse(any(n.startswith(".arka-convert-") for n in os.listdir(self.dir)))
        self.assertNotIn("broken.jpg", self.names())

    def test_one_bad_file_does_not_stop_the_rest(self):
        good = self.png("good.png")
        bad = self.dir / "bad.png"
        bad.write_bytes(b"nope")
        r = convert("jpg", str(bad), str(good))
        self.assertEqual(r.returncode, 1)
        self.assertTrue((self.dir / "good.jpg").exists())

    def test_refuses_wrong_requests(self):
        pic = self.png()
        self.assertEqual(convert("png", str(pic)).returncode, 1)          # already a PNG
        self.assertEqual(convert("mp3", str(pic)).returncode, 1)          # picture to audio
        self.assertEqual(convert("jpg", str(self.dir / "a.txt")).returncode, 1)
        self.assertEqual(convert().returncode, 2)

    def test_file_name_with_leading_dash_and_spaces(self):
        pic = self.png("-odd name.png")
        r = convert("jpg", str(pic))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("-odd name.jpg", self.names())

    def test_nothing_is_converted_without_the_sandbox(self):
        pic = self.png()
        env = dict(ENV, PATH="/nonexistent")
        r = subprocess.run([sys.executable, str(SCRIPT), "jpg", str(pic)], capture_output=True,
                           text=True, env=env)
        self.assertEqual(r.returncode, 1)
        self.assertEqual(self.names(), ["pic.png"])

    def test_converters_cannot_reach_the_network(self):
        server = socket.socket()
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        port = server.getsockname()[1]
        probe = ["python3", "-c",
                 f"import socket; socket.create_connection(('127.0.0.1', {port}), 2)"]
        control = subprocess.run(probe)
        self.assertEqual(control.returncode, 0, "the probe itself must work outside the sandbox")
        mod = load_script()
        prefix = mod.sandbox_prefix(self.dir)
        boxed = subprocess.run(prefix + probe, capture_output=True)
        server.close()
        self.assertNotEqual(boxed.returncode, 0, "the sandbox let a connection through")

    def test_sandbox_cannot_write_outside_its_scratch_folder(self):
        mod = load_script()
        scratch = self.dir / "scratch"
        scratch.mkdir()
        target = self.dir / "outside.txt"
        prefix = mod.sandbox_prefix(scratch)
        boxed = subprocess.run(prefix + ["sh", "-c", f"echo x > '{target}'"], capture_output=True)
        self.assertNotEqual(boxed.returncode, 0)
        self.assertFalse(target.exists())
        ok = subprocess.run(prefix + ["sh", "-c", f"echo x > '{scratch}/inside.txt'"], capture_output=True)
        self.assertEqual(ok.returncode, 0)


if __name__ == "__main__":
    unittest.main()
