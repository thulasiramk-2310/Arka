"""Boot-counter failure paths using fake commands; never accesses host boot state."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent.parent / "arka-arm-boot-counter"


class BootCounterTest(unittest.TestCase):
    def run_helper(self, **settings):
        with tempfile.TemporaryDirectory(prefix="arka-counter-test-") as tmp:
            root = Path(tmp)
            commands = {
                "grub2-editenv": '''
echo "grub $*" >> "$TEST_LOG"
if [ "$1" = list ]; then
    [ "${TEST_READ_FAIL:-0}" = 0 ] || exit 1
    [ "${TEST_STAGED:-1}" = 1 ] && echo 'greenboot_next_deployment_id=sha256:test'
    exit 0
fi
exit "${TEST_WRITE_FAIL:-0}"
''',
                "findmnt": '''
echo findmnt >> "$TEST_LOG"
[ "${TEST_FIND_FAIL:-0}" = 0 ] || exit 1
echo "${TEST_OPTIONS:-rw,relatime}"
''',
                "mount": '''
echo "mount $*" >> "$TEST_LOG"
case "$*" in
    '-o remount,rw /boot') exit "${TEST_RW_FAIL:-0}" ;;
    '-o remount,ro /boot') exit "${TEST_RO_FAIL:-0}" ;;
esac
exit 1
''',
                "mountpoint": '''
echo mountpoint >> "$TEST_LOG"
exit "${TEST_NOT_MOUNT:-0}"
''',
                # Prevent even a config read outside the isolated fixture.
                "sed": 'printf "%s\\n" "${TEST_MAX:-3}"',
            }
            for name, body in commands.items():
                p = root / name
                p.write_text("#!/bin/sh\n" + body)
                p.chmod(0o755)
            env = {k: v for k, v in os.environ.items() if not k.startswith("TEST_")}
            grubenv = root / "grubenv"
            if settings.pop("NO_GRUBENV", 0) != 1:
                grubenv.write_text("# GRUB Environment Block\n")
            env.update(PATH=str(root) + ":/usr/bin:/bin", TEST_LOG=str(root / "commands"), ARKA_GRUBENV=str(grubenv))
            (root / "commands").touch()
            env.update({"TEST_" + k: str(v) for k, v in settings.items()})
            result = subprocess.run(["sh", str(SCRIPT)], env=env, capture_output=True, text=True, timeout=5)
            return result, (root / "commands").read_text().splitlines()

    def test_no_staged_update_does_not_write_or_mount(self):
        r, calls = self.run_helper(STAGED=0)
        self.assertEqual(r.returncode, 0)
        self.assertEqual(calls, ["grub list"])

    def test_no_boot_environment_file_is_not_an_error(self):
        r, calls = self.run_helper(NO_GRUBENV=1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(calls, [])

    def test_boot_that_is_not_its_own_mount_needs_no_remount(self):
        r, calls = self.run_helper(NOT_MOUNT=1, FIND_FAIL=1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("findmnt", calls)
        self.assertIn("grub - set boot_counter=3 boot_success=0", calls)

    def test_read_failure_is_not_treated_as_no_update(self):
        r, calls = self.run_helper(READ_FAIL=1)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("cannot read", r.stderr)
        self.assertEqual(calls, ["grub list"])

    def test_writable_boot_needs_no_remount(self):
        r, calls = self.run_helper(MAX=5)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("grub - set boot_counter=5 boot_success=0", calls)
        self.assertFalse(any(c.startswith("mount ") for c in calls))

    def test_read_only_boot_is_restored_after_success(self):
        r, calls = self.run_helper(OPTIONS="ro,relatime")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(calls[-3:], ["mount -o remount,rw /boot", "grub - set boot_counter=3 boot_success=0", "mount -o remount,ro /boot"])

    def test_counter_failure_is_reported_and_boot_restored(self):
        for options in ["rw,relatime", "ro,relatime"]:
            with self.subTest(options=options):
                r, calls = self.run_helper(OPTIONS=options, WRITE_FAIL=1)
                self.assertNotEqual(r.returncode, 0)
                self.assertIn("cannot arm", r.stderr)
                self.assertNotIn("Armed", r.stdout)
                if options.startswith("ro"):
                    self.assertEqual(calls[-1], "mount -o remount,ro /boot")

    def test_mount_lookup_failure_prevents_counter_write(self):
        r, calls = self.run_helper(FIND_FAIL=1)
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse(any(c.startswith("grub - set") for c in calls))

    def test_rw_remount_failure_prevents_write_and_attempts_restore(self):
        r, calls = self.run_helper(OPTIONS="ro", RW_FAIL=1)
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse(any(c.startswith("grub - set") for c in calls))
        self.assertEqual(calls[-1], "mount -o remount,ro /boot")

    def test_ro_restore_failure_is_reported(self):
        r, _ = self.run_helper(OPTIONS="ro", RO_FAIL=1)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("cannot restore", r.stderr)

    def test_invalid_retry_counts_use_positive_default(self):
        for value in ["0", "-1", "garbage", ""]:
            with self.subTest(value=value):
                r, calls = self.run_helper(MAX=value)
                self.assertEqual(r.returncode, 0, r.stderr)
                self.assertIn("grub - set boot_counter=3 boot_success=0", calls)


if __name__ == "__main__":
    unittest.main()
