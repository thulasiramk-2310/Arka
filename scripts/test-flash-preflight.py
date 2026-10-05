#!/usr/bin/env python3
"""Exercise source validation without privileges, block devices or disk writes."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent / "flash-usb.sh"


class PreflightTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="arka-flash-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "calls"
        self.stub("id", "printf '0\\n'")
        self.stub("sgdisk", 'printf "%s\\n" "$*" >> "$CALL_LOG"; '
                  'if [ "$VALID_GPT" = yes ]; then echo "No problems found"; '
                  'else echo "CRC mismatch"; fi')
        for name in ("lsblk", "wipefs", "partprobe", "blockdev"):
            self.stub(name, f'echo "UNEXPECTED {name}" >> "$CALL_LOG"; exit 98')
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                        CALL_LOG=str(self.log), VALID_GPT="yes")
        self.image = self.root / "image.raw"
        self.image.write_bytes(b"\0" * 512 + b"EFI PART" + b"\0" * 3576)

    def stub(self, name, body):
        p = self.bin / name
        p.write_text("#!/bin/sh\n" + body + "\n")
        p.chmod(0o700)

    def run_script(self, image):
        result = subprocess.run(["bash", str(SCRIPT), str(image),
                                 str(self.root / "not-a-device")],
                                env=self.env, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("UNEXPECTED", self.log.read_text() if self.log.exists() else "")
        return result

    def test_symlink_resolves_to_full_image_before_device_access(self):
        link = self.root / "latest.raw"
        link.symlink_to(self.image.name)
        result = self.run_script(link)
        self.assertIn("not a block device", result.stderr)
        self.assertEqual(self.log.read_text().strip(), f"-v {self.image}")

    def test_empty_image_is_rejected(self):
        self.image.write_bytes(b"")
        self.assertIn("nonempty regular file", self.run_script(self.image).stderr)
        self.assertFalse(self.log.exists())

    def test_wrong_format_is_rejected_before_partition_tools(self):
        self.image.write_bytes(b"QFI" + b"x" * 4096)
        self.assertIn("no raw GPT header", self.run_script(self.image).stderr)
        self.assertFalse(self.log.exists())

    def test_damaged_source_gpt_is_rejected(self):
        self.env["VALID_GPT"] = "no"
        self.assertIn("source GPT is damaged", self.run_script(self.image).stderr)


if __name__ == "__main__":
    unittest.main()
