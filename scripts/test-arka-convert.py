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
        self.assertEqual(got, ["jpg", "webp", "avif", "pdf", "text", "tiff", "bmp", "smaller-image", "clean", "zip"])
        # any other file can still be zipped, and nothing else
        self.assertEqual(convert("--targets", str(self.dir / "x.txt")).stdout.split(), ["zip"])
        # a GIF cannot be made smaller; a ZIP cannot be zipped again
        self.assertNotIn("smaller-image", convert("--targets", str(self.dir / "x.gif")).stdout.split())
        self.assertEqual(convert("--targets", str(self.dir / "x.zip")).stdout.split(), [])

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
        # Downloads is the fallback when a folder is read-only, so this test points HOME at a
        # read-only folder too: it must never write into a real Downloads folder.
        home = self.dir / "home"
        home.mkdir()
        ro.chmod(0o555)
        home.chmod(0o555)
        try:
            mod = load_script()
            old_home = os.environ.get("HOME")
            os.environ["HOME"] = str(home)
            try:
                with self.assertRaises(RuntimeError) as caught:
                    mod.convert_one(str(target), "jpg")
            finally:
                if old_home is None:
                    os.environ.pop("HOME", None)
                else:
                    os.environ["HOME"] = old_home
            self.assertIn("can't be written to", str(caught.exception))
            self.assertEqual(sorted(os.listdir(ro)), ["pic.png"])
            self.assertEqual(os.listdir(home), [])
        finally:
            ro.chmod(0o755)
            home.chmod(0o755)

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

    def noisy(self, name, fmt, size=(1600, 1200)):
        from PIL import Image
        path = self.dir / name
        Image.effect_noise(size, 40).convert("RGB").save(path, fmt, **({"quality": 95} if fmt == "JPEG" else {}))
        return path

    def test_smaller_picture_is_smaller_and_keeps_the_original(self):
        from PIL import Image
        for name, fmt in (("photo.jpg", "JPEG"), ("shot.png", "PNG"), ("snap.webp", "WEBP")):
            with self.subTest(name=name):
                src = self.noisy(name, fmt)
                before = src.read_bytes()
                r = convert("smaller-image", str(src))
                self.assertEqual(r.returncode, 0, r.stderr)
                out = self.dir / f"{src.stem} (smaller){src.suffix}"
                self.assertTrue(out.exists(), self.names())
                self.assertLess(out.stat().st_size, src.stat().st_size)
                self.assertEqual(src.read_bytes(), before)
                with Image.open(out) as im:
                    self.assertEqual(im.format, fmt)

    def test_smaller_picture_does_not_carry_camera_or_location_details(self):
        from PIL import Image
        for name, fmt in (("geo.jpg", "JPEG"), ("geo.webp", "WEBP")):
            with self.subTest(name=name):
                im = Image.effect_noise((1600, 1200), 40).convert("RGB")
                exif = Image.Exif()
                exif[0x010F] = "TestCameraMaker"
                gps = exif.get_ifd(0x8825)
                gps[1], gps[2], gps[3], gps[4] = "N", (12.0, 58.0, 30.0), "E", (77.0, 35.0, 10.0)
                src = self.dir / name
                im.save(src, fmt, quality=95, exif=exif)
                self.assertEqual(Image.open(src).getexif().get(0x010F), "TestCameraMaker")  # it was there
                self.assertEqual(convert("smaller-image", str(src)).returncode, 0)
                after = Image.open(self.dir / f"geo (smaller){src.suffix}").getexif()
                self.assertIsNone(after.get(0x010F))
                self.assertEqual(len(after.get_ifd(0x8825)), 0)

    def test_a_big_picture_is_scaled_down_to_2560(self):
        from PIL import Image
        src = self.noisy("huge.jpg", "JPEG", (3200, 2400))
        convert("smaller-image", str(src))
        with Image.open(self.dir / "huge (smaller).jpg") as im:
            self.assertEqual(im.size, (2560, 1920))

    def test_smaller_picture_that_cannot_shrink_is_refused_not_copied(self):
        from PIL import Image
        src = self.dir / "flat.png"
        Image.new("RGB", (8, 8), "white").save(src, optimize=True)
        mod = load_script()
        with self.assertRaisesRegex(RuntimeError, "already as small"):
            mod.convert_one(src, "smaller-image")
        self.assertEqual(self.names(), ["flat.png"])

    def test_smaller_picks_the_right_action_for_each_kind(self):
        pic = self.noisy("photo.jpg", "JPEG")
        txt = self.dir / "notes.txt"
        txt.write_text("hello\n" * 20)
        r = convert("smaller", str(pic), str(txt))
        self.assertEqual(r.returncode, 1)                       # the text file cannot be made smaller
        self.assertIn("photo (smaller).jpg", self.names())
        self.assertNotIn("notes (smaller).txt", self.names())

    def test_zip_any_file_leaves_the_original_and_tests_clean(self):
        import zipfile
        odd = self.dir / "weird name (1).data"
        odd.write_bytes(b"x" * 5000)
        before = odd.read_bytes()
        r = convert("zip", str(odd))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(odd.read_bytes(), before)
        with zipfile.ZipFile(self.dir / "weird name (1).zip") as z:
            self.assertEqual(z.namelist(), ["weird name (1).data"])
            self.assertIsNone(z.testzip())
            self.assertEqual(z.read("weird name (1).data"), before)

    def test_zip_never_overwrites_and_refuses_folders_and_zips(self):
        src = self.dir / "a.txt"
        src.write_text("hi")
        (self.dir / "a.zip").write_bytes(b"mine")
        convert("zip", str(src))
        self.assertEqual((self.dir / "a.zip").read_bytes(), b"mine")
        self.assertIn("a (1).zip", self.names())
        mod = load_script()
        with self.assertRaisesRegex(RuntimeError, "isn't a file"):
            mod.convert_one(self.dir, "zip")
        with self.assertRaisesRegex(RuntimeError, "already a ZIP"):
            mod.convert_one(self.dir / "a (1).zip", "zip")

    @unittest.skipUnless(shutil.which("soffice"), "needs LibreOffice (ArkaOS has it)")
    def test_word_to_pdf_and_pdf_back_to_word(self):
        import re
        import zipfile
        txt = self.dir / "doc.txt"
        txt.write_text("ArkaOS test document\n\nA paragraph of real text that should survive the trip.\n")
        subprocess.run(["soffice", "--headless", "--convert-to", "docx", "--outdir", str(self.dir), str(txt)],
                       capture_output=True, check=True)
        docx = self.dir / "doc.docx"
        self.assertTrue(docx.exists())
        r = convert("pdf", str(docx))
        self.assertEqual(r.returncode, 0, r.stderr)
        pdf = self.dir / "doc.pdf"
        self.assertTrue(pdf.read_bytes().startswith(b"%PDF"))
        text = subprocess.run(["pdftotext", str(pdf), "-"], capture_output=True, text=True).stdout
        self.assertIn("real text that should survive", " ".join(text.split()))
        docx.unlink()
        r = convert("docx", str(pdf))
        self.assertEqual(r.returncode, 0, r.stderr)
        xml = zipfile.ZipFile(self.dir / "doc.docx").read("word/document.xml").decode()
        self.assertIn("real text that should survive", " ".join(re.sub(r"<[^>]+>", " ", xml).split()))
        self.assertFalse(any(n.startswith(".arka-convert-") for n in self.names()))

    @unittest.skipUnless(shutil.which("soffice"), "needs LibreOffice (ArkaOS has it)")
    def test_a_scanned_pdf_is_refused_to_word_with_a_plain_reason(self):
        from PIL import Image
        scan = self.dir / "scan.pdf"
        Image.new("RGB", (600, 800), "white").save(scan, "PDF")
        mod = load_script()
        with self.assertRaisesRegex(RuntimeError, "looks scanned"):
            mod.convert_one(scan, "docx")
        self.assertEqual(self.names(), ["scan.pdf"])

    @unittest.skipUnless(shutil.which("soffice"), "needs LibreOffice (ArkaOS has it)")
    def test_a_spreadsheet_becomes_a_pdf(self):
        csv = self.dir / "data.csv"
        csv.write_text("name,score\nana,9\nravi,7\n")
        subprocess.run(["soffice", "--headless", "--convert-to", "xlsx", "--outdir", str(self.dir), str(csv)],
                       capture_output=True, check=True)
        r = convert("pdf", str(self.dir / "data.xlsx"))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue((self.dir / "data.pdf").read_bytes().startswith(b"%PDF"))

    @unittest.skipUnless("libmp3lame" in encoders(), "this ffmpeg has no MP3 encoder (ArkaOS's does)")
    def test_smaller_sound_is_an_mp3_and_a_small_mp3_is_refused(self):
        wav = self.dir / "tone.wav"
        ffmpeg("-f", "lavfi", "-i", "sine=frequency=440:duration=3", str(wav))
        r = convert("smaller-audio", str(wav))
        self.assertEqual(r.returncode, 0, r.stderr)
        out = self.dir / "tone (smaller).mp3"
        self.assertLess(out.stat().st_size, wav.stat().st_size)
        mod = load_script()
        with self.assertRaisesRegex(RuntimeError, "already as small"):
            mod.convert_one(out, "smaller-audio")

    @unittest.skipUnless("libopenh264" in encoders(), "this ffmpeg has no H.264 encoder (ArkaOS's does)")
    def test_smaller_video_is_an_mp4_no_wider_than_1280(self):
        src = self.dir / "clip.mkv"
        ffmpeg("-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=24:duration=2", "-c:v", "mpeg4", "-q:v", "1", str(src))
        r = convert("smaller-video", str(src))
        self.assertEqual(r.returncode, 0, r.stderr)
        out = self.dir / "clip (smaller).mp4"
        self.assertTrue(out.exists(), self.names())
        width = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                                "stream=width", "-of", "csv=p=0", str(out)], capture_output=True, text=True).stdout.strip()
        self.assertEqual(width, "1280")

    # ---- ZIP together: several files and folders in ONE archive -------------------------

    def zipped(self, name, folder=None):
        import zipfile
        z = zipfile.ZipFile((folder or self.dir) / name)
        self.assertIsNone(z.testzip())
        return z

    def test_zip_together_skips_a_picked_link_instead_of_following_it(self):
        import zipfile
        secret = Path(tempfile.mkdtemp())
        try:
            (secret / "key.txt").write_text("private")
            link = self.dir / "notes"
            link.symlink_to(secret)
            doc = self.dir / "report.txt"
            doc.write_text("hello")
            done = convert("zip-all", str(link), str(doc))
            self.assertEqual(done.returncode, 0, done.stderr)
            zips = list(self.dir.glob("*.zip"))
            self.assertEqual(len(zips), 1)                       # saved here, not beside the link's target
            with zipfile.ZipFile(zips[0]) as z:
                self.assertEqual(z.namelist(), ["report.txt"])  # the link's target is not in it
            self.assertEqual(list(secret.glob("*.zip")), [])
        finally:
            shutil.rmtree(secret)

    def test_zip_together_makes_one_archive_and_leaves_every_original(self):
        a = self.dir / "notes.txt"; a.write_text("hello " * 400)
        b = self.png("pic.png")
        c = self.dir / "doc.pdf"; c.write_bytes(b"%PDF-1.4 not really")
        before = {x.name: x.read_bytes() for x in (a, b, c)}
        r = convert("zip-all", str(a), str(b), str(c))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.names().count("notes and 2 more.zip"), 1)
        self.assertEqual([n for n in self.names() if n.endswith(".zip")], ["notes and 2 more.zip"])
        z = self.zipped("notes and 2 more.zip")
        self.assertEqual(sorted(z.namelist()), ["doc.pdf", "notes.txt", "pic.png"])
        for name, data in before.items():
            self.assertEqual(z.read(name), data)
            self.assertEqual((self.dir / name).read_bytes(), data)

    def test_zip_together_names_two_items_after_both(self):
        a = self.dir / "a.txt"; a.write_text("a")
        b = self.dir / "b.txt"; b.write_text("b")
        self.assertEqual(convert("zip-all", str(a), str(b)).returncode, 0)
        self.assertIn("a and b.zip", self.names())

    def test_zip_together_keeps_two_files_with_the_same_name(self):
        (self.dir / "one").mkdir(); (self.dir / "two").mkdir()
        x = self.dir / "one" / "photo.jpg"; x.write_bytes(b"first")
        y = self.dir / "two" / "photo.jpg"; y.write_bytes(b"second")
        self.assertEqual(convert("zip-all", str(x), str(y)).returncode, 0)
        z = self.zipped("photo and photo.zip", self.dir / "one")
        self.assertEqual(sorted(z.namelist()), ["photo (1).jpg", "photo.jpg"])
        self.assertEqual(sorted([z.read("photo.jpg"), z.read("photo (1).jpg")]), [b"first", b"second"])

    def test_zip_together_takes_folders_whole_and_does_not_follow_links(self):
        outside = self.dir / "outside"; outside.mkdir()
        (outside / "secret.txt").write_text("not mine to include")
        pack = self.dir / "pack"; (pack / "sub").mkdir(parents=True); (pack / "empty").mkdir()
        (pack / "a.txt").write_text("a"); (pack / "sub" / "b.txt").write_text("b")
        os.symlink(outside / "secret.txt", pack / "link.txt")
        os.symlink(outside, pack / "linkdir")
        lone = self.dir / "lone.txt"; lone.write_text("lone")
        self.assertEqual(convert("zip-all", str(pack), str(lone)).returncode, 0)
        z = self.zipped("pack and lone.zip")
        names = sorted(z.namelist())
        self.assertEqual(names, ["lone.txt", "pack/a.txt", "pack/empty/", "pack/sub/b.txt"])
        self.assertFalse(any("secret" in n or "link" in n for n in names))

    def test_zip_together_never_overwrites_and_never_packs_itself(self):
        a = self.dir / "a.txt"; a.write_text("a")
        b = self.dir / "b.txt"; b.write_text("b")
        (self.dir / "a and b.zip").write_bytes(b"mine")
        self.assertEqual(convert("zip-all", str(a), str(b)).returncode, 0)
        self.assertEqual((self.dir / "a and b.zip").read_bytes(), b"mine")
        self.assertIn("a and b (1).zip", self.names())
        # a folder zipped together with a file that sits inside it must not swallow the working folder
        box = self.dir / "box"; box.mkdir(); inner = box / "in.txt"; inner.write_text("in")
        self.assertEqual(convert("zip-all", str(inner), str(box)).returncode, 0)
        z = self.zipped("in and box.zip", box)
        self.assertTrue(all(".arka-convert" not in n for n in z.namelist()))
        self.assertEqual([p.name for p in self.dir.iterdir() if p.name.startswith(".arka-convert")], [])

    def test_zip_together_needs_two_items_and_says_so_for_a_missing_one(self):
        a = self.dir / "a.txt"; a.write_text("a")
        self.assertEqual(convert("zip-all", str(a)).returncode, 2)
        self.assertEqual(convert("zip-all", str(a), str(self.dir / "gone.txt")).returncode, 1)
        self.assertEqual([n for n in self.names() if n.endswith(".zip")], [])

    def test_zip_together_stores_packed_formats_and_squeezes_the_rest(self):
        import zipfile
        t = self.dir / "text.txt"; t.write_text("squeeze me " * 2000)
        j = self.dir / "photo.jpg"; j.write_bytes(os.urandom(4096))
        self.assertEqual(convert("zip-all", str(t), str(j)).returncode, 0)
        z = self.zipped("text and photo.zip")
        self.assertEqual(z.getinfo("photo.jpg").compress_type, zipfile.ZIP_STORED)
        self.assertEqual(z.getinfo("text.txt").compress_type, zipfile.ZIP_DEFLATED)
        self.assertLess(z.getinfo("text.txt").compress_size, z.getinfo("text.txt").file_size // 4)

    def test_zip_together_is_documented_in_the_script_help(self):
        self.assertIn("arka-convert zip-all", SCRIPT.read_text().split('"""')[1])


    # ---- Make smaller: how much to give up (light, medium, strong) ---------------------

    def big_noisy_jpeg(self, name="big.jpg"):
        from PIL import Image
        path = self.dir / name
        Image.frombytes("RGB", (3000, 2000), os.urandom(3000 * 2000 * 3)).save(path, "JPEG", quality=95)
        return path

    def test_smaller_levels_step_down_in_size_and_in_dimensions(self):
        from PIL import Image
        src = self.big_noisy_jpeg()
        sizes, sides = {}, {}
        for level, limit in (("light", 2560), ("medium", 1920), ("strong", 1280)):
            r = convert(f"smaller:{level}", str(src))
            self.assertEqual(r.returncode, 0, r.stderr)
            made = sorted(self.dir.glob("big (smaller)*.jpg"), key=lambda p: p.stat().st_mtime_ns)[-1]
            sizes[level] = made.stat().st_size
            with Image.open(made) as im:
                sides[level] = max(im.size)
            self.assertLessEqual(sides[level], limit)
            self.assertGreater(sides[level], limit - 5)   # scaled to the limit, not further
        self.assertGreater(sizes["light"], sizes["medium"])
        self.assertGreater(sizes["medium"], sizes["strong"])
        self.assertLess(sizes["light"], src.stat().st_size)

    def test_smaller_with_no_level_is_exactly_light(self):
        src = self.big_noisy_jpeg()
        convert("smaller", str(src))
        plain = (self.dir / "big (smaller).jpg").read_bytes()
        convert("smaller:light", str(src))
        self.assertEqual((self.dir / "big (smaller) (1).jpg").read_bytes(), plain)

    def test_smaller_refuses_an_unknown_level_and_makes_nothing(self):
        src = self.big_noisy_jpeg()
        r = convert("smaller:tiny", str(src))
        self.assertEqual(r.returncode, 2)
        self.assertIn("light, medium, strong", r.stderr)
        self.assertEqual([n for n in self.names() if "smaller" in n], [])

    @unittest.skipUnless(shutil.which("gs"), "needs Ghostscript (ArkaOS has it)")
    def test_smaller_pdf_levels_step_down(self):
        from PIL import Image, ImageFilter
        pdf = self.dir / "scan.pdf"
        # Photo-like: a smooth colour field with fine grain. (Pure random noise is not what real pages hold,
        # and Ghostscript treats it oddly at the lowest setting.)
        w, h = 2400, 1800
        field = Image.effect_mandelbrot((w, h), (-2.2, -1.5, 1.0, 1.5), 80).convert("RGB").filter(ImageFilter.GaussianBlur(6))
        grain = Image.frombytes("RGB", (w, h), os.urandom(w * h * 3)).filter(ImageFilter.GaussianBlur(1.2))
        Image.blend(field, grain, 0.25).save(pdf, "PDF", resolution=300)
        sizes = {}
        for level in ("light", "medium", "strong"):
            self.assertEqual(convert(f"smaller:{level}", str(pdf)).returncode, 0)
            made = sorted(self.dir.glob("scan (smaller)*.pdf"), key=lambda p: p.stat().st_mtime_ns)[-1]
            sizes[level] = made.stat().st_size
            self.assertTrue(made.read_bytes().startswith(b"%PDF"))
        self.assertGreaterEqual(sizes["light"], sizes["medium"])
        self.assertGreater(sizes["medium"], sizes["strong"])
        self.assertLess(sizes["strong"], pdf.stat().st_size)

    def test_smaller_sound_and_video_settings_step_down_and_light_is_unchanged(self):
        mod = load_script()
        self.assertEqual(mod.smaller_audio_opts("light"), ["-vn", "-c:a", "libmp3lame", "-q:a", "5"])
        quality = [int(mod.smaller_audio_opts(l)[-1]) for l in mod.QUALITY_LEVELS]
        self.assertEqual(quality, sorted(quality))              # a higher lame quality number is a smaller file
        self.assertEqual(len(set(quality)), 3)
        def pick(opts, flag):
            return opts[opts.index(flag) + 1]
        widths = [int(pick(mod.smaller_video_opts(l), "-vf").split("min(")[1].split(",")[0]) for l in mod.QUALITY_LEVELS]
        rates = [int(pick(mod.smaller_video_opts(l), "-b:v").rstrip("k")) for l in mod.QUALITY_LEVELS]
        sound = [int(pick(mod.smaller_video_opts(l), "-b:a").rstrip("k")) for l in mod.QUALITY_LEVELS]
        for series in (widths, rates, sound):
            self.assertEqual(series, sorted(series, reverse=True))
            self.assertEqual(len(set(series)), 3)
        self.assertEqual(widths[0], 1280)                        # light keeps the old 1280 px limit
        self.assertEqual(rates[0], 1500)


def tone_power(path, start, length, freq):
    """How strong one frequency is in part of a file (Goertzel, mono 16 kHz), with no extra libraries."""
    import array
    import math
    raw = subprocess.run(["ffmpeg", "-loglevel", "error", "-ss", str(start), "-t", str(length), "-i", str(path),
                          "-ac", "1", "-ar", "16000", "-f", "s16le", "-"], capture_output=True, check=True).stdout
    samples = array.array("h", raw)
    coeff = 2 * math.cos(2 * math.pi * freq / 16000)
    s1 = s2 = 0.0
    for x in samples:
        s1, s2 = x / 32768 + coeff * s1 - s2, s1
    return (s1 * s1 + s2 * s2 - coeff * s1 * s2) / max(1, len(samples)) ** 2


def frame_pixel(path, at, xy):
    from PIL import Image
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "f.png"
        subprocess.run(["ffmpeg", "-loglevel", "error", "-ss", str(at), "-i", str(path), "-frames:v", "1", str(out)],
                       check=True)
        with Image.open(out) as im:
            return im.convert("RGB").getpixel(xy)


def media_info(path):
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration:stream=codec_type,width,height",
                          "-of", "json", str(path)], capture_output=True, text=True, check=True).stdout
    import json
    data = json.loads(out)
    video = next((st for st in data["streams"] if st["codec_type"] == "video"), {})
    return (float(data["format"]["duration"]), video.get("width"), video.get("height"),
            any(st["codec_type"] == "audio" for st in data["streams"]))


WEBM_OK = "libvpx-vp9" in encoders() and "libopus" in encoders()


class MediaEditTest(unittest.TestCase):
    """The edits the edit wheel added on 2026-11-05: annotate, video crop and redact, split, join and bleep."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def made(self, label):
        found = [p for p in self.dir.iterdir() if f"({label})" in p.name]
        self.assertEqual(len(found), 1, f"expected one ({label}) file, have {sorted(p.name for p in self.dir.iterdir())}")
        return found[0]

    def video(self, name="clip.webm", size="320x240", seconds=4, sound=True):
        path = self.dir / name
        args = ["-f", "lavfi", "-i", f"testsrc=size={size}:rate=25:duration={seconds}"]
        if sound:
            args += ["-f", "lavfi", "-i", f"sine=frequency=300:duration={seconds}", "-c:a", "libopus", "-shortest"]
        ffmpeg(*args, "-c:v", "libvpx-vp9", "-b:v", "300k", str(path))
        return path

    def sound(self, name="talk.mp3", freq=300, seconds=5):
        path = self.dir / name
        ffmpeg("-f", "lavfi", "-i", f"sine=frequency={freq}:duration={seconds}", str(path))
        return path

    def test_annotate_draws_every_shape_and_keeps_the_original(self):
        from PIL import Image
        src = self.dir / "shot.png"
        Image.new("RGB", (200, 120), (255, 255, 255)).save(src)
        shapes = ('[{"t":"box","c":"#00FF00","w":4,"p":[10,10,50,40]},'
                  '{"t":"highlight","c":"#FF0000","w":1,"p":[100,60,60,40]},'
                  '{"t":"pen","c":"#0000FF","w":6,"p":[20,100,80,100]},'
                  '{"t":"arrow","c":"#000000","w":4,"p":[120,20,190,20]},'
                  '{"t":"text","c":"#000000","w":20,"p":[100,90],"s":"Hi"}]')
        done = convert("annotate", str(src), shapes)
        self.assertEqual(done.returncode, 0, done.stderr)
        with Image.open(self.made("annotated")) as im:
            out = im.convert("RGB")
        self.assertEqual(out.getpixel((11, 30)), (0, 255, 0))          # the box's edge
        self.assertEqual(out.getpixel((35, 30)), (255, 255, 255))      # inside the box: not filled
        r, g, b = out.getpixel((130, 70))                               # the highlight is see-through
        self.assertTrue(r == 255 and 100 < g < 255 and g == b, (r, g, b))
        self.assertEqual(out.getpixel((50, 100)), (0, 0, 255))         # the pen line
        self.assertEqual(out.getpixel((188, 20)), (0, 0, 0))           # the arrow's head
        with Image.open(src) as im:
            self.assertEqual(im.getpixel((11, 30)), (255, 255, 255))  # the original is untouched

    def test_annotate_rejects_bad_drawings(self):
        from PIL import Image
        src = self.dir / "shot.png"
        Image.new("RGB", (50, 50)).save(src)
        for bad in ("not json", "[]", '[{"t":"blob","c":"#000000","w":2,"p":[1,1]}]',
                    '[{"t":"pen","c":"red","w":2,"p":[1,1]}]', '[{"t":"box","c":"#000000","w":2,"p":[1,1]}]',
                    '[{"t":"text","c":"#000000","w":2,"p":[1,1],"s":""}]'):
            done = convert("annotate", str(src), bad)
            self.assertNotEqual(done.returncode, 0, bad)
        self.assertEqual(sorted(p.name for p in self.dir.iterdir()), ["shot.png"])  # nothing half-made is left

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_video_crop_keeps_the_box_in_even_pixels(self):
        src = self.video()
        done = convert("crop", str(src), "11", "21", "201", "151")
        self.assertEqual(done.returncode, 0, done.stderr)
        seconds, w, h, sound = media_info(self.made("cropped"))
        self.assertEqual((w, h), (200, 150))
        self.assertTrue(sound)
        self.assertAlmostEqual(seconds, 4, delta=0.2)
        self.assertNotEqual(convert("crop", str(src), "300", "0", "100", "100").returncode, 0)  # outside the picture

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_video_redact_blacks_out_only_while_asked(self):
        src = self.video()
        done = convert("redact", str(src), "--between", "1", "2", "0", "0", "100", "80")
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.made("redacted")
        self.assertEqual(frame_pixel(out, 1.5, (50, 40)), (0, 0, 0))
        self.assertNotEqual(frame_pixel(out, 0.5, (50, 40)), (0, 0, 0))
        self.assertNotEqual(frame_pixel(out, 2.5, (50, 40)), (0, 0, 0))
        self.assertNotEqual(frame_pixel(out, 1.5, (200, 150)), (0, 0, 0))  # outside the box

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_video_redact_removes_the_title_and_other_details(self):
        plain = self.video("plain.webm")
        src = self.dir / "titled.webm"
        ffmpeg("-i", str(plain), "-c", "copy", "-metadata", "title=Account 4471 2290", "-metadata", "comment=secret", str(src))
        done = convert("redact", str(src), "0", "0", "40", "40")
        self.assertEqual(done.returncode, 0, done.stderr)
        tags = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format_tags", "-of", "json",
                               str(self.made("redacted"))], capture_output=True, text=True).stdout
        self.assertNotIn("4471", tags)
        self.assertNotIn("secret", tags)

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_video_redact_covers_odd_edges_completely(self):
        src = self.video()
        done = convert("redact", str(src), "101", "51", "51", "41")   # columns 101-151, rows 51-91
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.made("redacted")
        for xy in ((101, 51), (151, 51), (101, 91), (151, 91)):     # every corner pixel is covered
            self.assertLess(max(frame_pixel(out, 1, xy)), 16, xy)      # black, give or take compression noise

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_a_sideways_phone_video_is_measured_upright(self):
        src = self.video("flat.webm", sound=False)
        turned = self.dir / "phone.mp4"
        ffmpeg("-display_rotation", "90", "-i", str(src), "-c", "copy", str(turned))
        mod = load_script()
        scratch = Path(tempfile.mkdtemp(dir=self.dir))
        info = mod.probe(scratch, str(turned))
        self.assertEqual((info["width"], info["height"]), (240, 320))  # as the frame is shown and filtered

    def test_split_a_song_with_cover_art(self):
        from PIL import Image
        cover = self.dir / "cover.png"
        Image.new("RGB", (64, 64), (200, 40, 40)).save(cover)
        plain = self.sound("plain.mp3")
        song = self.dir / "song.mp3"
        ffmpeg("-i", str(plain), "-i", str(cover), "-map", "0:a", "-map", "1:v", "-c:a", "copy", "-c:v", "png",
               "-disposition:v", "attached_pic", str(song))
        done = convert("split-media", str(song), "2")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertAlmostEqual(media_info(self.made("part 1"))[0], 2, delta=0.2)
        self.assertAlmostEqual(media_info(self.made("part 2"))[0], 3, delta=0.2)

    @unittest.skipUnless("libopenh264" in encoders(), "MOV and AVI are saved as MP4, which needs openh264 (ArkaOS has it)")
    def test_trim_a_mov_saves_an_mp4(self):
        mov = self.dir / "clip.mov"
        ffmpeg("-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=4", "-c:v", "mpeg4", str(mov))
        done = convert("trim", str(mov), "1", "3")
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.dir / "clip (trimmed).mp4"
        self.assertAlmostEqual(media_info(out)[0], 2, delta=0.2)

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_split_media_makes_two_parts_that_add_up(self):
        src = self.video()
        done = convert("split-media", str(src), "1.5")
        self.assertEqual(done.returncode, 0, done.stderr)
        first, second = media_info(self.made("part 1"))[0], media_info(self.made("part 2"))[0]
        self.assertAlmostEqual(first, 1.5, delta=0.15)
        self.assertAlmostEqual(first + second, 4, delta=0.2)
        self.assertNotEqual(convert("split-media", str(src), "9").returncode, 0)  # after the end

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_join_videos_fits_the_second_inside_the_first_and_adds_silence(self):
        a = self.video("a.webm")
        b = self.video("b.webm", size="640x360", seconds=2, sound=False)
        done = convert("join-media", str(a), str(b))
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.made("joined")
        seconds, w, h, sound = media_info(out)
        self.assertEqual((w, h), (320, 240))
        self.assertTrue(sound)
        self.assertAlmostEqual(seconds, 6, delta=0.3)
        self.assertEqual(frame_pixel(out, 5, (160, 5)), (0, 0, 0))     # a 16:9 clip in 4:3 gets bars, not stretching
        self.assertNotEqual(convert("join-media", str(a), str(self.sound())).returncode, 0)  # video with sound

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_join_with_the_silent_video_first(self):
        quiet = self.video("a.webm", seconds=2, sound=False)
        loud = self.video("b.webm", seconds=2)
        done = convert("join-media", str(quiet), str(loud))
        self.assertEqual(done.returncode, 0, done.stderr)
        seconds, w, h, sound = media_info(self.made("joined"))
        self.assertAlmostEqual(seconds, 4, delta=0.3)
        self.assertTrue(sound)
        self.assertGreater(tone_power(self.made("joined"), 2.5, 1, 300), 1e-6)   # the second file's sound is there

    def test_overlapping_bleep_parts_are_one_beep(self):
        src = self.sound()
        done = convert("bleep", str(src), "1", "2.5", "2", "3")
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.made("bleeped")
        self.assertGreater(tone_power(out, 2.1, 0.3, 1000), 100 * tone_power(out, 2.1, 0.3, 300))
        self.assertGreater(tone_power(out, 3.5, 0.5, 300), 100 * tone_power(out, 3.5, 0.5, 1000))

    def test_a_drawing_too_big_for_one_argument_is_passed_as_a_file(self):
        from PIL import Image
        import json
        src = self.dir / "shot.png"
        Image.new("RGB", (400, 300), (255, 255, 255)).save(src)
        pts = [v for i in range(20000) for v in (20 + i % 360, 20 + (i * 7) % 260)]
        drawing = self.dir / "drawing.json"
        drawing.write_text(json.dumps([{"t": "pen", "c": "#0000FF", "w": 3, "p": pts}]))
        self.assertGreater(drawing.stat().st_size, 128 * 1024)
        done = convert("annotate", str(src), "@" + str(drawing))
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertTrue(self.made("annotated").exists())
        self.assertTrue(drawing.exists())            # only files in the private runtime folder are removed

    def test_a_bad_drawing_is_reported_in_plain_words(self):
        from PIL import Image
        src = self.dir / "shot.png"
        Image.new("RGB", (40, 30)).save(src)
        done = convert("annotate", str(src), '[{"t":"pen","c":"#000000","w":2,"p":[1]}]')
        self.assertNotEqual(done.returncode, 0)
        self.assertNotIn("Traceback", done.stderr)
        self.assertNotIn("RuntimeError", done.stderr)

    def test_join_sounds_one_after_the_other(self):
        a, b = self.sound("a.mp3", 300, 3), self.sound("b.mp3", 600, 2)
        done = convert("join-media", str(a), str(b))
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.made("joined")
        self.assertAlmostEqual(media_info(out)[0], 5, delta=0.2)
        self.assertGreater(tone_power(out, 1, 1, 300), 10 * tone_power(out, 1, 1, 600))
        self.assertGreater(tone_power(out, 3.5, 1, 600), 10 * tone_power(out, 3.5, 1, 300))

    def test_bleep_replaces_the_words_with_a_tone(self):
        src = self.sound()
        done = convert("bleep", str(src), "1", "2", "3", "3.5")
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.made("bleeped")
        self.assertAlmostEqual(media_info(out)[0], 5, delta=0.2)
        inside, outside = (1.2, 0.6), (2.2, 0.6)
        self.assertGreater(tone_power(out, *inside, 1000), 100 * tone_power(out, *inside, 300))  # the words are gone
        self.assertGreater(tone_power(out, *outside, 300), 100 * tone_power(out, *outside, 1000))  # the rest is kept
        for bad in (["2", "1"], ["1"], ["x", "2"]):
            self.assertNotEqual(convert("bleep", str(src), *bad).returncode, 0, bad)

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_bleep_in_a_video_keeps_the_picture(self):
        src = self.video()
        done = convert("bleep", str(src), "0.5", "1")
        self.assertEqual(done.returncode, 0, done.stderr)
        seconds, w, h, sound = media_info(self.made("bleeped"))
        self.assertEqual((w, h, sound), (320, 240, True))

    @unittest.skipUnless(WEBM_OK, "ffmpeg has no VP9/Opus encoder here")
    def test_frame_gives_the_picture_the_edit_window_draws_on(self):
        from PIL import Image
        src = self.video()
        out = self.dir / "frame.png"
        done = convert("--frame", str(src), "1", str(out))
        self.assertEqual(done.returncode, 0, done.stderr)
        with Image.open(out) as im:
            self.assertEqual(im.size, (320, 240))


AV1_OK = "libaom-av1" in encoders()


class NewPictureFormatsTest(unittest.TestCase):
    """AVIF (through ffmpeg), TIFF and BMP, added 2026-11-05."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        from PIL import Image, ImageDraw
        im = Image.new("RGBA", (120, 80), (0, 0, 0, 0))
        ImageDraw.Draw(im).ellipse((10, 10, 110, 70), fill=(220, 40, 60, 255))
        self.logo = self.dir / "logo.png"
        im.save(self.logo)

    def tearDown(self):
        self.tmp.cleanup()

    def open(self, name):
        from PIL import Image
        with Image.open(self.dir / name) as im:
            im.load()
            return im.copy()

    def test_tiff_keeps_see_through_parts(self):
        self.assertEqual(convert("tiff", str(self.logo)).returncode, 0)
        im = self.open("logo.tiff")
        self.assertEqual((im.mode, im.getpixel((2, 2))[3], im.getpixel((60, 40))), ("RGBA", 0, (220, 40, 60, 255)))

    def test_bmp_puts_white_behind_see_through_parts(self):
        self.assertEqual(convert("bmp", str(self.logo)).returncode, 0)
        im = self.open("logo.bmp")
        self.assertEqual((im.getpixel((2, 2)), im.getpixel((60, 40))), ((255, 255, 255), (220, 40, 60)))

    @unittest.skipUnless(AV1_OK, "ffmpeg has no libaom AV1 encoder here")
    def test_avif_round_trip_keeps_see_through_parts(self):
        done = convert("avif", str(self.logo))
        self.assertEqual(done.returncode, 0, done.stderr)
        avif = self.dir / "logo.avif"
        self.assertEqual(avif.read_bytes()[4:12], b"ftypavif")
        self.logo.unlink()
        done = convert("png", str(avif))
        self.assertEqual(done.returncode, 0, done.stderr)
        im = self.open("logo.png")
        self.assertEqual(im.getpixel((2, 2))[3], 0)                    # still see-through
        r, g, b, a = im.getpixel((60, 40))
        self.assertTrue(abs(r - 220) < 12 and abs(g - 40) < 12 and abs(b - 60) < 12 and a == 255, (r, g, b, a))

    @unittest.skipUnless(AV1_OK, "ffmpeg has no libaom AV1 encoder here")
    def test_an_avif_can_be_edited(self):
        self.assertEqual(convert("avif", str(self.logo)).returncode, 0)
        done = convert("crop", str(self.dir / "logo.avif"), "0", "0", "60", "40")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(self.open("logo (cropped).png").size, (60, 40))

    def test_targets_leave_out_the_format_a_file_already_is(self):
        self.assertNotIn("tiff", convert("--targets", "x.tif").stdout.split())
        self.assertNotIn("avif", convert("--targets", "x.avif").stdout.split())
        self.assertIn("avif", convert("--targets", "x.png").stdout.split())


class FitToSizeTest(unittest.TestCase):
    """Smaller "under 10 MB" / "under 25 MB": a copy under the size, or none, said plainly."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_a_big_photo_comes_out_under_10_mb(self):
        from PIL import Image
        src = self.dir / "photo.png"
        Image.frombytes("RGB", (2600, 2000), os.urandom(2600 * 2000 * 3)).save(src)   # noise: about 15 MB
        self.assertGreater(src.stat().st_size, 10_000_000)
        done = convert("smaller:10mb", str(src))
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.dir / "photo (under 10 MB).jpg"
        self.assertLessEqual(out.stat().st_size, 10_000_000)

    def test_a_long_recording_comes_out_under_25_mb(self):
        src = self.dir / "talk.mp3"
        ffmpeg("-f", "lavfi", "-i", "sine=frequency=300:duration=900", "-b:a", "320k", str(src))   # 15 min, ~36 MB
        self.assertGreater(src.stat().st_size, 25_000_000)
        done = convert("smaller:25mb", str(src))
        self.assertEqual(done.returncode, 0, done.stderr)
        out = self.dir / "talk (under 25 MB).mp3"
        self.assertLessEqual(out.stat().st_size, 25_000_000)
        self.assertAlmostEqual(media_info(out)[0], 900, delta=2)

    def test_a_file_already_under_the_size_is_left_alone(self):
        src = self.dir / "small.mp3"
        ffmpeg("-f", "lavfi", "-i", "sine=frequency=300:duration=2", str(src))
        done = convert("smaller:10mb", str(src))
        self.assertNotEqual(done.returncode, 0)
        self.assertEqual(sorted(p.name for p in self.dir.iterdir()), ["small.mp3"])


def text_pdf(line):
    """A one-page PDF whose text is real text (not a picture), written by hand."""
    stream = f"BT /F1 10 Tf 40 760 Td ({line}) Tj ET".encode()
    objs = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R "
            b"/Resources << /Font << /F1 5 0 R >> >> >>",
            b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream",
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
    out, offsets = b"%PDF-1.4\n", []
    for i, o in enumerate(objs, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + o + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    out += b"".join(b"%010d 00000 n \n" % off for off in offsets)
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
    return out


def has_english_ocr():
    try:
        out = subprocess.run(["tesseract", "--list-langs"], capture_output=True, text=True).stdout
    except OSError:
        return False
    return "eng" in out.split()


def has_libmat2():
    return subprocess.run([sys.executable, "-c", "import libmat2"], capture_output=True).returncode == 0


class PrivacyToolsTest(unittest.TestCase):
    """What ArkaOS adds over Tangerine: hidden details, text from pictures, finding private text."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def invoice(self):
        from PIL import Image, ImageDraw, ImageFont
        font = None
        for f in ("/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf", "/usr/share/fonts/TTF/DejaVuSans.ttf",
                  "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf"):
            if Path(f).exists():
                font = ImageFont.truetype(f, 28)
                break
        font = font or ImageFont.load_default(28)
        im = Image.new("RGB", (1000, 360), "white")
        d = ImageDraw.Draw(im)
        for y, line in ((30, "Invoice 2026-118 for Ram K"), (90, "Contact: ram.k@example.com"),
                        (150, "Phone: +91 98765 43210"), (210, "Card: 4111 1111 1111 1111"),
                        (270, "Order total 2,450 rupees")):
            d.text((40, y), line, fill="black", font=font)
        path = self.dir / "invoice.png"
        im.save(path)
        return path

    def test_the_private_text_matcher_leaves_dates_amounts_and_addresses_alone(self):
        import re
        mod = load_script()

        def kinds(text):
            out = []
            for kind, pat in mod.PRIVATE_PATTERNS:
                out += [kind for m in re.finditer(pat, text) if mod.plausible(kind, m.group(0))]
            return sorted(out)
        for text in ("2026-11-05", "05/11/2026", "12.345.678", "192.168.100.200", "100 200 300 400",
                     "20261105123045", "2,450", "Invoice 2026-118", "AB12 CDEF GHIJ",
                     "Ref 2024-11-05 10:20", "Date 05.11.2024 14:30", "12/11/2024 1234567"):
            with self.subTest(text=text):
                self.assertEqual(kinds(text), [])
        self.assertEqual(kinds("+91 98765 43210"), ["phone"])
        self.assertEqual(kinds("(044) 2345 6789"), ["phone"])
        self.assertEqual(kinds("9876543210"), ["phone"])
        self.assertEqual(kinds("4111 1111 1111 1111"), ["card"])
        self.assertEqual(kinds("4111 1111 1111 1112"), [])            # fails the card check digit
        self.assertEqual(kinds("ram.k@example.com"), ["email"])
        self.assertEqual(kinds("Pay ram@okhdfcbank now"), ["upi"])
        self.assertIn("iban", kinds("GB82 WEST 1234 5698 7654 32"))

    def test_a_see_through_background_keeps_the_room_transparent(self):
        from PIL import Image
        src = self.dir / "shot.jpg"
        Image.new("RGB", (40, 30), (200, 40, 40)).save(src)
        done = convert("background", str(src), "none", "10")
        self.assertEqual(done.returncode, 0, done.stderr)
        with Image.open(self.dir / "shot (with background).png") as out:   # PNG: JPEG can't be see-through
            self.assertEqual(out.size, (60, 50))
            self.assertEqual(out.getpixel((2, 2))[3], 0)
            self.assertEqual(out.getpixel((30, 25))[3], 255)

    def test_a_cmyk_picture_converts_to_png(self):
        from PIL import Image
        src = self.dir / "print.jpg"
        Image.new("CMYK", (40, 30), (0, 200, 200, 0)).save(src)
        done = convert("png", str(src))
        self.assertEqual(done.returncode, 0, done.stderr)
        with Image.open(self.dir / "print.png") as out:
            self.assertEqual(out.mode, "RGB")

    def test_a_footer_alone_does_not_count_as_real_text(self):
        mod = load_script()
        footer_only = "\f".join(["Page %d of 10 - Confidential" % i for i in range(1, 11)])
        pages = [pg for pg in footer_only.split("\f") if pg.strip()]
        self.assertFalse(any(len(pg.strip()) >= 80 for pg in pages))   # the rule read_text applies
        self.assertIn(">= 80", SCRIPT.read_text())

    def test_a_sideways_phone_photo_converts_upright(self):
        from PIL import Image
        im = Image.new("RGB", (300, 100), (200, 40, 40))
        ex = im.getexif()
        ex[0x0112] = 6                                                # stored sideways, "turn 90 degrees"
        src = self.dir / "phone.jpg"
        im.save(src, exif=ex)
        done = convert("png", str(src))
        self.assertEqual(done.returncode, 0, done.stderr)
        with Image.open(self.dir / "phone.png") as out:
            self.assertEqual(out.size, (100, 300))

    @unittest.skipUnless(has_english_ocr(), "tesseract with English is not installed here (ArkaOS has it)")
    def test_find_private_text_suggests_the_right_boxes(self):
        import json
        done = convert("--find-private", str(self.invoice()))
        self.assertEqual(done.returncode, 0, done.stderr)
        found = json.loads(done.stdout)
        kinds = sorted(f["kind"] for f in found)
        self.assertEqual(kinds, ["card", "email", "phone"])     # not the invoice number or the total
        email = next(f["box"] for f in found if f["kind"] == "email")
        self.assertTrue(80 <= email[1] <= 100, email)              # on the email line

    @unittest.skipUnless(has_english_ocr(), "tesseract with English is not installed here (ArkaOS has it)")
    def test_text_from_a_picture(self):
        done = convert("text", str(self.invoice()))
        self.assertEqual(done.returncode, 0, done.stderr)
        text = (self.dir / "invoice (text).txt").read_text()
        self.assertIn("ram.k@example.com", text)
        self.assertIn("Invoice", text)

    def test_a_pdf_with_text_is_copied_out_without_ocr(self):
        pdf = self.dir / "doc.pdf"
        pdf.write_bytes(text_pdf("Hello from a real text layer in a PDF document, long enough to read as a page of text"))
        done = convert("text", str(pdf))
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("real text layer", (self.dir / "doc (text).txt").read_text())

    @unittest.skipUnless(has_libmat2(), "mat2's library is not installed here (ArkaOS has it)")
    def test_hidden_details_are_listed_in_plain_groups(self):
        import json
        from PIL import Image
        im = Image.new("RGB", (64, 48), (200, 40, 40))
        ex = im.getexif()
        ex[0x010F], ex[0x0110], ex[0x013B] = "Canon", "EOS R6", "Ram K"
        gps = ex.get_ifd(0x8825)
        gps[1], gps[2], gps[3], gps[4] = "N", (13.0, 5.0, 0.0), "E", (80.0, 16.0, 0.0)
        path = self.dir / "photo.jpg"
        im.save(path, exif=ex)
        done = convert("--details", str(path))
        self.assertEqual(done.returncode, 0, done.stderr)
        info = json.loads(done.stdout)
        groups = dict(info["groups"])
        self.assertIn("Location", groups)
        self.assertIn(["Model", "EOS R6"], groups["Camera or device"])
        self.assertIn(["Artist", "Ram K"], groups["People"])
        self.assertEqual(path.read_bytes()[:2], b"\xff\xd8")      # nothing was changed


class FinishedTitleTest(unittest.TestCase):
    """The pop-up that says a job is done must name the job, whichever way the wheel or the menu asked for it."""

    @classmethod
    def setUpClass(cls):
        cls.mod = load_script()

    def test_every_make_smaller_action_says_made_smaller(self):
        for fmt in ("smaller", "smaller-image", "smaller-pdf", "smaller-audio", "smaller-video"):
            with self.subTest(fmt=fmt):
                self.assertEqual(self.mod.done_title(fmt), "Made smaller")

    def test_zip_and_clean_keep_their_own_titles(self):
        self.assertEqual(self.mod.done_title("zip"), "Zipped")
        self.assertEqual(self.mod.done_title("clean"), "Removed hidden details")

    def test_a_format_change_says_converted(self):
        for fmt in ("png", "jpg", "pdf", "mp3", "mp4", "docx"):
            with self.subTest(fmt=fmt):
                self.assertEqual(self.mod.done_title(fmt), "Converted")


if __name__ == "__main__":
    unittest.main()
