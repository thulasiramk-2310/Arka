#!/usr/bin/env python3
"""Check the dock start script against accounts made by older ArkaOS images, offline.

    python3 -I scripts/test-dock-start.py

An account keeps the dock settings it was given on its first login, and the script never replaces them.
So when an app's launcher is renamed in the image, accounts made earlier still pin the old name, and the
dock shows the pinned old entry and the running app as two icons. These tests build such accounts and
run the real script (with a stand-in dock and settings tool) to see what it leaves behind.
"""
import os
import re
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "crystal-dock" / "arka-dock-start"
DEFAULT = (Path(__file__).resolve().parent.parent / "crystal-dock" / "panel_1.conf").read_text()


class DockStart(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="arka-dock-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.home = self.root / "home"
        self.skel = self.root / "skel"
        for d in ("KDE", "generic"):
            (self.skel / ".config/crystal-dock" / d).mkdir(parents=True)
            (self.skel / ".config/crystal-dock" / d / "panel_1.conf").write_text(DEFAULT)
            (self.skel / ".config/crystal-dock" / d / "appearance.conf").write_text("[General]\n")
        (self.root / "bin").mkdir()
        for name in ("kwriteconfig6", "crystal-dock"):
            p = self.root / "bin" / name
            p.write_text("#!/bin/sh\nexit 0\n")
            p.chmod(p.stat().st_mode | stat.S_IEXEC)
        text = SCRIPT.read_text()
        text = text.replace("/etc/skel", str(self.skel)).replace("exec /usr/bin/crystal-dock", f"exec {self.root}/bin/crystal-dock")
        self.script = self.root / "start"
        self.script.write_text(text)

    def account(self, launchers=None, desktop="KDE"):
        """Make an account that already has dock settings, with the given launcher list."""
        d = self.home / ".config/crystal-dock" / desktop
        d.mkdir(parents=True)
        conf = DEFAULT if launchers is None else re.sub(r'^launchers=.*$', f'launchers="{launchers}"', DEFAULT, flags=re.M)
        (d / "panel_1.conf").write_text(conf)
        (d / "appearance.conf").write_text("[General]\n")
        return d / "panel_1.conf"

    def start(self):
        env = dict(os.environ, HOME=str(self.home), PATH=f"{self.root}/bin:/usr/bin:/bin")
        r = subprocess.run(["sh", str(self.script)], env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(r.returncode, 0, r.stderr)

    @staticmethod
    def launchers(conf):
        m = re.search(r'^launchers="([^"]*)"', conf.read_text(), re.M)
        return m.group(1).split(";") if m else None

    def test_an_old_pin_is_renamed_so_the_dock_shows_one_icon(self):
        conf = self.account("org.kde.dolphin;org.mozilla.firefox;org.kde.konsole;org.kde.discover;arka-dashboard;org.kde.kcalc;systemsettings")
        self.start()
        self.assertEqual(self.launchers(conf), ["org.kde.dolphin", "org.mozilla.firefox", "org.kde.konsole",
                                                 "org.kde.discover", "org.arka.dashboard", "org.kde.kcalc", "systemsettings"])

    def test_every_renamed_app_is_covered(self):
        names = "bluetooth capsule dashboard hotkeys perms settings sound update wifi".split()
        conf = self.account(";".join(f"arka-{n}" for n in names))
        self.start()
        self.assertEqual(self.launchers(conf), [f"org.arka.{n}" for n in names])

    def test_a_pin_listed_under_both_names_becomes_one(self):
        conf = self.account("org.kde.dolphin;arka-dashboard;org.arka.dashboard;org.kde.kcalc")
        self.start()
        self.assertEqual(self.launchers(conf), ["org.kde.dolphin", "org.arka.dashboard", "org.kde.kcalc"])

    def test_the_users_own_choices_are_kept_in_order(self):
        conf = self.account("org.kde.kate;arka-dashboard;org.gimp.GIMP;org.kde.dolphin")
        self.start()
        self.assertEqual(self.launchers(conf), ["org.kde.kate", "org.arka.dashboard", "org.gimp.GIMP", "org.kde.dolphin"])

    def test_an_account_that_removed_the_dashboard_does_not_get_it_back(self):
        conf = self.account("org.kde.dolphin;org.kde.kcalc")
        self.start()
        self.assertEqual(self.launchers(conf), ["org.kde.dolphin", "org.kde.kcalc"])

    def test_a_current_config_is_left_exactly_as_it_is(self):
        conf = self.account()
        before = conf.read_text()
        self.start()
        self.assertEqual(conf.read_text(), before)

    def test_running_it_twice_changes_nothing_more(self):
        conf = self.account("arka-dashboard;arka-wifi")
        self.start()
        once = conf.read_text()
        self.start()
        self.assertEqual(conf.read_text(), once)

    def test_both_dock_profiles_are_fixed(self):
        a = self.account("arka-dashboard", "KDE")
        b = self.account("arka-dashboard", "generic")
        self.start()
        self.assertEqual(self.launchers(a), ["org.arka.dashboard"])
        self.assertEqual(self.launchers(b), ["org.arka.dashboard"])

    def test_other_settings_in_the_file_are_untouched(self):
        conf = self.account("arka-dashboard")
        extra = conf.read_text() + "\nsomethingElse=keep me\n"
        conf.write_text(extra)
        self.start()
        self.assertIn("somethingElse=keep me", conf.read_text())

    def test_a_new_account_is_seeded_with_the_current_names(self):
        self.start()
        self.assertEqual(self.launchers(self.home / ".config/crystal-dock/KDE/panel_1.conf"),
                         self.launchers(self.skel / ".config/crystal-dock/KDE/panel_1.conf"))


if __name__ == "__main__":
    import sys
    sys.exit(unittest.main(verbosity=1))
