#!/usr/bin/env python3
"""Exercise arka-rtc-mode with fake firmware boot entries and a fake timedatectl.

No clock, service or system configuration is changed.
Run: python3 scripts/test-rtc-mode.py
"""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent.parent / "arka-rtc-mode"
GUID = "8be4df61-93ca-11d2-aa0d-00e098032b8c"

FAKE_TIMEDATECTL = r'''#!/bin/sh
echo "$@" >> "$ARKA_TEST_CALLS"
[ "${ARKA_TEST_FAIL:-}" = 1 ] && exit 1
case "$1" in
    show) echo "${ARKA_TEST_LOCALRTC:-no}" ;;
esac
'''


def utf16(text):
    return text.encode("utf-16-le")


class RtcModeTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.efi = self.root / "efivars"
        self.efi.mkdir()
        self.mark = self.root / "checked"
        self.calls = self.root / "calls"
        fake = self.root / "timedatectl"
        fake.write_text(FAKE_TIMEDATECTL)
        fake.chmod(0o755)
        self.fake = fake

    def tearDown(self):
        self.tmp.cleanup()

    def entry(self, number, name):
        # 4 attribute bytes, then the load option: attributes, path length, name.
        data = b"\x07\x00\x00\x00" + b"\x01\x00\x00\x00" + b"\x04\x00" + utf16(name) + b"\x00\x00" + b"\x01\x02"
        (self.efi / f"Boot{number:04X}-{GUID}").write_bytes(data)

    def run_script(self, **env):
        e = dict(os.environ, ARKA_EFIVARS=str(self.efi), ARKA_RTC_MARK=str(self.mark),
                 ARKA_TIMEDATECTL=str(self.fake), ARKA_TEST_CALLS=str(self.calls), **env)
        return subprocess.run(["sh", str(SCRIPT)], env=e, capture_output=True, text=True)

    def recorded(self):
        return self.calls.read_text().splitlines() if self.calls.exists() else []

    def test_windows_found_switches_to_local_time(self):
        self.entry(1, "Fedora")
        self.entry(5, "Windows Boot Manager")
        r = self.run_script()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("set-local-rtc 1 --adjust-system-clock", self.recorded())
        self.assertTrue(self.mark.exists())

    def test_no_windows_leaves_the_clock_alone(self):
        self.entry(1, "Fedora")
        self.entry(2, "UEFI: PXE IPv4 Realtek PCIe GBE Family Controller")
        r = self.run_script()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(any(c.startswith("set-local-rtc") for c in self.recorded()))
        self.assertTrue(self.mark.exists())

    def test_already_local_time_is_not_changed_again(self):
        self.entry(5, "Windows Boot Manager")
        r = self.run_script(ARKA_TEST_LOCALRTC="yes")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(any(c.startswith("set-local-rtc") for c in self.recorded()))

    def test_decides_only_once(self):
        self.entry(5, "Windows Boot Manager")
        self.mark.write_text("")
        r = self.run_script()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.recorded(), [])

    def test_failure_is_retried_next_boot(self):
        self.entry(5, "Windows Boot Manager")
        r = self.run_script(ARKA_TEST_FAIL="1")
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse(self.mark.exists())

    def test_not_uefi_does_nothing(self):
        self.efi.rmdir()
        r = self.run_script()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.recorded(), [])
        self.assertTrue(self.mark.exists())


if __name__ == "__main__":
    unittest.main()
