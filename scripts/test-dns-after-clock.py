#!/usr/bin/env python3
"""Exercise arka-dns-after-clock with a fake clock, fake timedatectl and fake systemctl.

Nothing real is restarted. Run: python3 scripts/test-dns-after-clock.py
"""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent.parent / "arka-dns-after-clock"

# Prints the next line of $T/times each call (the last line repeats).
FAKE_NOW = r'''#!/bin/sh
f="$ARKA_TEST_DIR/times"; n="$ARKA_TEST_DIR/n"
i=$(cat "$n" 2>/dev/null || echo 0); i=$((i + 1)); echo $i > "$n"
l=$(wc -l < "$f"); [ "$i" -gt "$l" ] && i=$l
sed -n "${i}p" "$f"
'''
# Reports "no" for the first ARKA_TEST_UNSYNCED calls, then "yes".
FAKE_TIMEDATECTL = r'''#!/bin/sh
c="$ARKA_TEST_DIR/tdc"; i=$(cat "$c" 2>/dev/null || echo 0); i=$((i + 1)); echo $i > "$c"
if [ "$i" -le "${ARKA_TEST_UNSYNCED:-0}" ]; then echo no; else echo yes; fi
'''
FAKE_SYSTEMCTL = r'''#!/bin/sh
echo "$@" >> "$ARKA_TEST_DIR/systemctl.log"
'''


class DnsAfterClockTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        for name, body in (("now", FAKE_NOW), ("timedatectl", FAKE_TIMEDATECTL), ("systemctl", FAKE_SYSTEMCTL)):
            p = self.dir / name
            p.write_text(body)
            p.chmod(0o755)

    def tearDown(self):
        self.tmp.cleanup()

    def run_script(self, times, unsynced=0, limit=360):
        (self.dir / "times").write_text("\n".join(str(t) for t in times) + "\n")
        env = dict(os.environ, ARKA_TEST_DIR=str(self.dir), ARKA_NOW=str(self.dir / "now"),
                   ARKA_TIMEDATECTL=str(self.dir / "timedatectl"), ARKA_SYSTEMCTL=str(self.dir / "systemctl"),
                   ARKA_SLEEP="true", ARKA_TEST_UNSYNCED=str(unsynced), ARKA_LIMIT=str(limit))
        return subprocess.run(["sh", str(SCRIPT)], env=env, capture_output=True, text=True)

    def restarts(self):
        f = self.dir / "systemctl.log"
        return f.read_text().splitlines() if f.exists() else []

    def test_large_clock_jump_restarts_name_resolution(self):
        # Start at 1.78e9 (a month behind), 6 polls later the clock reads weeks ahead.
        r = self.run_script([1_788_000_000, 1_791_000_000], unsynced=6)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.restarts(), ["restart systemd-resolved"])

    def test_small_difference_does_nothing(self):
        # 6 polls of 5 s = 30 s waited, clock advanced 31 s: no jump.
        r = self.run_script([1_791_000_000, 1_791_000_031], unsynced=6)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.restarts(), [])

    def test_already_synchronised_does_nothing(self):
        r = self.run_script([1_791_000_000, 1_791_000_000], unsynced=0)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.restarts(), [])

    def test_never_synchronised_gives_up_without_restarting(self):
        r = self.run_script([1_791_000_000, 1_791_000_900], unsynced=10_000, limit=3)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.restarts(), [])
        self.assertIn("never synchronised", r.stdout)

    def test_clock_stepped_backwards_also_restarts(self):
        r = self.run_script([1_791_000_000, 1_788_000_000], unsynced=2)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.restarts(), ["restart systemd-resolved"])


if __name__ == "__main__":
    unittest.main()
