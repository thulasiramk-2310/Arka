#!/usr/bin/env python3
"""Check the Meta+D (Show Desktop) replacement, offline.

    python3 -I scripts/test-showdesktop.py

KDE's own Show Desktop hides the dock together with the windows. The ArkaOS version is a small KWin script
that minimizes the ordinary windows instead, plus a login step that hands the Meta+D key to it. These tests
read the package files, run the script's real logic against pretend windows (needs node, skipped without it),
and run the login step with a stand-in shortcut service.
"""
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PKG = ROOT / "kwin-scripts" / "arka-showdesktop"
KEYS = ROOT / "kwin-scripts" / "arka-showdesktop-keys"
AUTOSTART = ROOT / "autostart" / "arka-showdesktop-keys.desktop"
MAIN_JS = PKG / "contents" / "code" / "main.js"
NODE = shutil.which("node") or shutil.which("nodejs")


class Package(unittest.TestCase):
    def setUp(self):
        self.meta = json.loads((PKG / "metadata.json").read_text())

    def test_it_is_a_kwin_script_package_named_after_its_folder(self):
        self.assertEqual(self.meta["KPackageStructure"], "KWin/Script")
        self.assertEqual(self.meta["KPlugin"]["Id"], PKG.name)
        self.assertEqual(self.meta["X-Plasma-API"], "javascript")
        self.assertTrue((PKG / "contents" / self.meta["X-Plasma-MainScript"]).is_file())

    def test_it_is_on_without_anyone_switching_it_on(self):
        self.assertIs(self.meta["KPlugin"].get("EnabledByDefault"), True)

    def test_it_takes_meta_d_and_never_uses_kdes_show_desktop(self):
        js = MAIN_JS.read_text()
        self.assertIn('registerShortcut("ArkaOS Show Desktop"', js)
        self.assertIn('"Meta+D"', js)
        # showingDesktop is the call that hides the dock; the script must only minimize windows.
        self.assertNotIn("showingDesktop", js)
        self.assertIn("minimized", js)


@unittest.skipUnless(NODE, "node is not installed")
class Logic(unittest.TestCase):
    """Run main.js with a pretend KWin: windows with normalWindow/minimized/desktops, and registerShortcut."""

    HARNESS = r"""
const fs = require('fs');
const code = fs.readFileSync(process.argv[2], 'utf8');
const scenario = JSON.parse(process.argv[3]);
const current = 'D1';
const windows = scenario.windows.map(w => Object.assign({
    normalWindow: true, minimized: false, onAllDesktops: false, desktops: [current]}, w));
let handler = null, registered = null;
const workspace = { currentDesktop: current, windowList: () => windows };
function registerShortcut(title, text, keys, fn) { registered = [title, keys]; handler = fn; }
new Function('workspace', 'registerShortcut', code)(workspace, registerShortcut);
const out = { registered, steps: [] };
for (const step of scenario.steps) {
    if (step.close !== undefined) {
        Object.defineProperty(windows[step.close], 'minimized', { set() { throw new Error('closed'); }, get() { return true; } });
    } else if (step.user_minimizes !== undefined) {
        windows[step.user_minimizes].minimized = true;
    } else { handler(); }
    out.steps.push(windows.map(w => w.minimized));
}
console.log(JSON.stringify(out));
"""

    def run_scenario(self, windows, steps):
        r = subprocess.run([NODE, "-e", self.HARNESS, "x", str(MAIN_JS), json.dumps({"windows": windows, "steps": steps})],
                           capture_output=True, text=True, timeout=20)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads(r.stdout)

    def test_registers_meta_d(self):
        out = self.run_scenario([{}], [])
        self.assertEqual(out["registered"], ["ArkaOS Show Desktop", "Meta+D"])

    def test_first_press_puts_every_window_away_and_second_brings_them_back(self):
        out = self.run_scenario([{}, {}, {}], [{}, {}])
        self.assertEqual(out["steps"], [[True, True, True], [False, False, False]])

    def test_a_window_the_user_had_already_minimized_stays_minimized(self):
        out = self.run_scenario([{}, {"minimized": True}], [{}, {}])
        self.assertEqual(out["steps"], [[True, True], [False, True]])

    def test_panels_and_docks_are_left_alone(self):
        out = self.run_scenario([{}, {"normalWindow": False}], [{}])
        self.assertEqual(out["steps"], [[True, False]])

    def test_windows_on_other_desktops_are_left_alone(self):
        out = self.run_scenario([{}, {"desktops": ["D2"]}, {"onAllDesktops": True, "desktops": []}], [{}])
        self.assertEqual(out["steps"], [[True, False, True]])

    def test_a_window_closed_meanwhile_does_not_stop_the_others_coming_back(self):
        out = self.run_scenario([{}, {}], [{}, {"close": 0}, {}])
        self.assertEqual(out["steps"][2][1], False)

    def test_with_no_windows_nothing_breaks_and_the_next_press_works(self):
        out = self.run_scenario([], [{}, {}])
        self.assertEqual(out["steps"], [[], []])


class LoginStep(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="arka-showdesktop-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.home = self.root / "home"
        self.home.mkdir()
        (self.root / "bin").mkdir()
        fake = self.root / "bin" / "busctl"
        fake.write_text('#!/bin/sh\necho "$@" >> "$FAKE_LOG"\n'
                        '[ "$FAKE_FAIL" = 1 ] && exit 1\necho "ai 0"\nexit 0\n')
        fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
        sleep = self.root / "bin" / "sleep"
        sleep.write_text("#!/bin/sh\nexit 0\n")
        sleep.chmod(sleep.stat().st_mode | stat.S_IEXEC)
        self.log = self.root / "calls"

    def run_step(self, fail=False):
        env = dict(os.environ, HOME=str(self.home), PATH=f"{self.root}/bin:/usr/bin:/bin",
                   FAKE_LOG=str(self.log), FAKE_FAIL="1" if fail else "0")
        return subprocess.run(["sh", str(KEYS)], env=env, capture_output=True, text=True, timeout=20)

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def set_shortcut_calls(self):
        return [c for c in self.calls() if "setShortcut" in c]

    def test_it_frees_kdes_meta_d_and_gives_it_to_the_arkaos_action(self):
        self.assertEqual(self.run_step().returncode, 0)
        calls = self.set_shortcut_calls()
        self.assertEqual(len(calls), 2)
        self.assertIn("org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel setShortcut", calls[0])
        self.assertIn("kwin Show Desktop KWin Show Desktop 0 6", calls[0])        # no keys left on KDE's own action
        self.assertIn("kwin ArkaOS Show Desktop KWin", calls[1])
        self.assertIn(" 1 268435524 6", calls[1])                                  # Meta+D = Qt::META | Qt::Key_D

    def test_it_runs_once_per_account(self):
        self.run_step()
        self.run_step()
        self.assertEqual(len(self.set_shortcut_calls()), 2)

    def test_a_failure_is_not_remembered_so_the_next_login_tries_again(self):
        self.assertNotEqual(self.run_step(fail=True).returncode, 0)
        self.assertEqual(list(self.home.glob(".config/arka-showdesktop-keys")), [])
        failed_calls = len(self.set_shortcut_calls())
        self.assertGreater(failed_calls, 1)              # it kept trying while the service was down
        self.assertEqual(self.run_step().returncode, 0)  # the next login: works, and remembers
        self.assertEqual(len(self.set_shortcut_calls()), failed_calls + 2)
        self.assertTrue((self.home / ".config/arka-showdesktop-keys").exists())

    def test_a_change_the_user_makes_later_is_not_undone(self):
        self.run_step()
        before = len(self.calls())
        self.run_step()
        self.assertEqual(len(self.calls()), before)


class Installed(unittest.TestCase):
    def test_the_image_installs_all_three_files_where_the_autostart_entry_points(self):
        cf = (ROOT / "Containerfile").read_text()
        self.assertIn("COPY kwin-scripts/arka-showdesktop /usr/share/kwin/scripts/arka-showdesktop", cf)
        self.assertIn("COPY kwin-scripts/arka-showdesktop-keys /usr/libexec/arka-showdesktop-keys", cf)
        self.assertIn("COPY autostart/arka-showdesktop-keys.desktop /etc/xdg/autostart/arka-showdesktop-keys.desktop", cf)
        self.assertIn("Exec=/usr/libexec/arka-showdesktop-keys", AUTOSTART.read_text())
        self.assertIn("OnlyShowIn=KDE;", AUTOSTART.read_text())

    def test_the_login_step_is_executable_source(self):
        self.assertTrue(KEYS.read_text().startswith("#!/bin/sh"))


if __name__ == "__main__":
    sys.exit(unittest.main(verbosity=1))
