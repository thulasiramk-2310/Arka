#!/usr/bin/env python3
"""Exercise arka-convert with real conversions inside the real sandbox.

Needs bubblewrap, ffmpeg, poppler-utils and Pillow, the same tools ArkaOS ships.
Files are made in a temporary folder; nothing else on the system is touched.
Run: python3 scripts/test-arka-convert.py
"""

import os
from pathlib import Path
import shutil
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
        self.assertEqual(got, ["jpg", "webp", "pdf", "clean"])
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

    @unittest.skipUnless(shutil.which("gs"), "needs Ghostscript (ArkaOS has it)")
    def test_pdf_with_big_pictures_gets_smaller(self):
        from PIL import Image
        import random
        rnd = random.Random(7)
        big = Image.frombytes("RGB", (2400, 1800), bytes(rnd.getrandbits(8) for _ in range(2400 * 1800 * 3)))
        pdf = self.dir / "scan.pdf"
        big.save(pdf, "PDF", resolution=300)
        r = convert("smaller-pdf", str(pdf))
        self.assertEqual(r.returncode, 0, r.stderr)
        small = self.dir / "scan (smaller).pdf"
        self.assertTrue(small.exists())
        self.assertLess(small.stat().st_size, pdf.stat().st_size)
        self.assertTrue(small.read_bytes().startswith(b"%PDF"))

    @unittest.skipUnless(shutil.which("gs"), "needs Ghostscript (ArkaOS has it)")
    def test_pdf_that_cannot_shrink_is_left_alone(self):
        from PIL import Image
        pdf = self.dir / "tiny.pdf"
        Image.new("RGB", (8, 8), "white").save(pdf, "PDF")
        r = convert("smaller-pdf", str(pdf))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(self.names(), ["tiny.pdf"])

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

    def test_clean_is_offered_only_where_mat2_works(self):
        self.assertIn("clean", convert("--targets", str(self.dir / "a.mp4")).stdout.split())
        self.assertNotIn("clean", convert("--targets", str(self.dir / "a.mkv")).stdout.split())
        mkv = self.dir / "a.mkv"
        mkv.write_bytes(b"x")
        r = convert("clean", str(mkv))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(self.names(), ["a.mkv"])

    @unittest.skipUnless(shutil.which("mat2"), "needs mat2 (ArkaOS has it)")
    def test_clean_removes_camera_and_author_from_a_photo(self):
        from PIL import Image
        photo = self.dir / "photo.jpg"
        exif = Image.Exif()
        exif[0x010F] = "SecretCamCo"
        exif[0x013B] = "Alice Author"
        Image.new("RGB", (64, 48), (200, 30, 30)).save(photo, exif=exif)
        r = convert("clean", str(photo))
        self.assertEqual(r.returncode, 0, r.stderr)
        cleaned = self.dir / "photo (cleaned).jpg"
        with Image.open(cleaned) as im:
            self.assertFalse(dict(im.getexif()), "EXIF survived")
        self.assertIn(b"SecretCamCo", photo.read_bytes())  # the original is untouched

    def test_progress_lines_become_fractions(self):
        mod = load_script()
        self.assertEqual(mod.progress_fraction("out_time_us=5000000", 10.0), 0.5)
        self.assertEqual(mod.progress_fraction("out_time_ms=20000000\n", 10.0), 1.0)  # clamped
        self.assertIsNone(mod.progress_fraction("frame=12", 10.0))
        self.assertIsNone(mod.progress_fraction("out_time_us=N/A", 10.0))
        self.assertIsNone(mod.progress_fraction("out_time_us=100", None))

    def test_video_conversion_reports_progress(self):
        mkv = self.dir / "long.mkv"
        ffmpeg("-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=6",
               "-f", "lavfi", "-i", "sine=duration=6", "-shortest", str(mkv))
        seen = []
        made = load_script().convert_one(str(mkv), "webm", seen.append)
        self.assertEqual([p.name for p in made], ["long.webm"])
        self.assertTrue(seen, "no progress was reported")
        self.assertEqual(seen, sorted(seen))
        self.assertGreater(seen[-1], 0.8)
        self.assertFalse(any(n.startswith(".arka-convert-") for n in os.listdir(self.dir)))

    def test_a_stopped_conversion_leaves_nothing_behind(self):
        import signal
        import time
        mkv = self.dir / "long.mkv"
        ffmpeg("-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30:duration=60",
               "-c:v", "libvpx-vp9", "-deadline", "realtime", "-cpu-used", "8", str(mkv))
        proc = subprocess.Popen([sys.executable, str(SCRIPT), "webm", str(mkv)], env=ENV)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline and not any(n.startswith(".arka-convert-") for n in os.listdir(self.dir)):
            time.sleep(0.1)
        time.sleep(1.5)  # ffmpeg is now encoding
        proc.send_signal(signal.SIGTERM)
        proc.wait(timeout=20)
        self.assertNotEqual(proc.returncode, 0)
        self.assertEqual(self.names(), ["long.mkv"])
        self.assertFalse(any(n.startswith(".arka-convert-") for n in os.listdir(self.dir)))

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

    def pdf(self, name, pages):
        from PIL import Image
        ims = [Image.new("RGB", (80, 60), c) for c in ("white", "black", "red")[:pages]]
        path = self.dir / name
        ims[0].save(path, "PDF", save_all=True, append_images=ims[1:])
        return path

    def pages(self, path):
        out = subprocess.run(["pdfinfo", str(path)], capture_output=True, text=True).stdout
        return int(next(l for l in out.splitlines() if l.startswith("Pages:")).split()[1])

    def test_merge_pdf_joins_in_the_order_given_and_keeps_originals(self):
        a, b = self.pdf("a.pdf", 3), self.pdf("b.pdf", 1)
        r = convert("merge-pdf", str(a), str(b))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.names(), ["a (merged).pdf", "a.pdf", "b.pdf"])
        self.assertEqual(self.pages(self.dir / "a (merged).pdf"), 4)
        self.assertEqual(convert("merge-pdf", str(a)).returncode, 2)          # needs two
        self.assertEqual(convert("merge-pdf", str(a), str(self.png())).returncode, 1)

    def test_split_pdf_makes_one_file_per_page(self):
        doc = self.pdf("doc.pdf", 3)
        r = convert("split-pdf", str(doc))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.names(), ["doc page 1.pdf", "doc page 2.pdf", "doc page 3.pdf", "doc.pdf"])
        self.assertEqual(self.pages(self.dir / "doc page 2.pdf"), 1)
        self.assertEqual(convert("split-pdf", str(self.pdf("one.pdf", 1))).returncode, 1)

    def test_trim_keeps_the_asked_length_and_rejects_bad_times(self):
        src = self.dir / "tone.mp3"
        ffmpeg("-f", "lavfi", "-i", "sine=frequency=440:duration=10", str(src))
        r = convert("trim", str(src), "2", "5")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.names(), ["tone (trimmed).mp3", "tone.mp3"])
        length = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0",
                                 str(self.dir / "tone (trimmed).mp3")], capture_output=True, text=True).stdout
        self.assertAlmostEqual(float(length), 3.0, delta=0.1)
        for start, end in (("5", "2"), ("abc", "3"), ("-1", "3"), ("nan", "3"), ("1:00", "2:00")):
            self.assertEqual(convert("trim", str(src), start, end).returncode, 1, (start, end))
        self.assertEqual(convert("trim", str(self.png()), "1", "2").returncode, 1)
        self.assertEqual(len(self.names()), 3)                                # nothing else was left behind

    def gradient(self, name="g.png"):
        from PIL import Image
        im = Image.new("RGB", (200, 100))
        for x in range(200):
            for y in range(100):
                im.putpixel((x, y), (x, y, 128))
        path = self.dir / name
        im.save(path)
        return path

    def size_of(self, name):
        from PIL import Image
        with Image.open(self.dir / name) as im:
            return im.size

    def test_rotate_and_crop_make_copies_of_the_right_size(self):
        pic = self.gradient()
        self.assertEqual(convert("rotate", str(pic), "left").returncode, 0)
        self.assertEqual(self.size_of("g (rotated).png"), (100, 200))
        self.assertEqual(convert("rotate", str(pic), "up").returncode, 1)
        self.assertEqual(convert("crop", str(pic), "10", "10", "50", "40").returncode, 0)
        self.assertEqual(self.size_of("g (cropped).png"), (50, 40))
        for box in (("180", "10", "50", "40"), ("0", "0", "0", "5"), ("a", "b", "c", "d"), ("-1", "0", "5", "5")):
            self.assertEqual(convert("crop", str(pic), *box).returncode, 1, box)
        self.assertEqual(sorted(p.name for p in self.dir.iterdir() if p.name.startswith(".")), [])

    def test_redact_replaces_the_pixels_and_leaves_the_rest(self):
        from PIL import Image
        pic = self.gradient()
        self.assertEqual(convert("redact", str(pic), "0", "0", "20", "20", "100", "50", "30", "30").returncode, 0)
        with Image.open(self.dir / "g (redacted).png") as im:
            self.assertEqual(im.getpixel((5, 5)), (0, 0, 0))
            self.assertEqual(im.getpixel((110, 60)), (0, 0, 0))
            self.assertEqual(im.getpixel((150, 90)), (150, 90, 128))
        self.assertEqual(convert("redact", str(pic), "0", "0", "20").returncode, 2)       # a box is four numbers

    def test_redact_is_black_in_every_picture_mode(self):
        from PIL import Image
        for mode, name in (("CMYK", "cmyk.jpg"), ("L", "grey.png"), ("P", "pal.png"), ("RGBA", "rgba.png")):
            im = Image.new("RGB", (60, 40), (250, 240, 230)).convert(mode)
            im.save(self.dir / name)
            self.assertEqual(convert("redact", str(self.dir / name), "0", "0", "30", "20").returncode, 0, mode)
            out = next(p for p in self.dir.iterdir() if p.name.startswith(Path(name).stem + " (redacted)"))
            with Image.open(out) as got:
                px = got.convert("RGB").getpixel((10, 10))
                self.assertLess(max(px), 12, (mode, px))                    # black, not white
                self.assertGreater(min(got.convert("RGB").getpixel((50, 30))), 200, mode)  # the rest is untouched

    def test_background_pads_and_checks_its_arguments(self):
        from PIL import Image
        pic = self.gradient()
        self.assertEqual(convert("background", str(pic), "#1E90FF", "20").returncode, 0)
        self.assertEqual(self.size_of("g (with background).png"), (240, 140))
        with Image.open(self.dir / "g (with background).png") as im:
            self.assertEqual(im.getpixel((2, 2))[:3], (30, 144, 255))
        self.assertEqual(convert("background", str(pic), "#112233-#aabbcc", "30").returncode, 0)
        for bad in (("blue", "10"), ("#112233", "99999"), ("#112233", "-5")):
            self.assertEqual(convert("background", str(pic), *bad).returncode, 1, bad)
        self.assertEqual(convert("crop", str(self.dir / "a.txt"), "1", "1", "2", "2").returncode, 1)

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

    def test_read_only_folder_gives_a_plain_reason(self):
        ro = self.dir / "ro"
        ro.mkdir()
        pic = self.png("pic.png")
        target = ro / "pic.png"
        target.write_bytes(pic.read_bytes())
        ro.chmod(0o555)
        try:
            mod = load_script()
            with self.assertRaises(RuntimeError) as caught:
                mod.convert_one(str(target), "jpg")
            self.assertIn("can't be written to", str(caught.exception))
            self.assertEqual(sorted(os.listdir(ro)), ["pic.png"])
        finally:
            ro.chmod(0o755)

    def test_unicode_and_spaces_in_names(self):
        pic = self.png("ünïcödé фото 写真.png")
        r = convert("webp", str(pic))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("ünïcödé фото 写真.webp", self.names())

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
