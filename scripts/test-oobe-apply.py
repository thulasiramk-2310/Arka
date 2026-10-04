#!/usr/bin/env python3
"""Exercise the deployed helper with fake account commands and temporary paths.

No host accounts, system configuration, services, or disks are changed.
Run: python3 scripts/test-oobe-apply.py
"""

import fcntl
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


MOCK_COMMAND = r'''#!/usr/bin/python3
import json, os
from pathlib import Path
import sys
name = Path(sys.argv[0]).name
root = Path(os.environ['OOBE_TEST_ROOT'])
with (root / 'calls').open('a') as log:
    log.write(json.dumps([name, *sys.argv[1:]]) + '\n')
if name == os.environ.get('OOBE_TEST_FAIL'):
    sys.exit(1)
if name == 'id':
    user = sys.argv[-1]
    existing = {'root', 'arkasetup', 'daemon'}
    existing.update((root / 'accounts').read_text().splitlines())
    sys.exit(0 if user in existing else 1)
if name == 'useradd':
    with (root / 'accounts').open('a') as accounts:
        accounts.write(sys.argv[-1] + '\n')
if name == 'chpasswd':
    (root / 'password-input').write_text(sys.stdin.read())
'''


class ApplyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='arka-oobe-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ('run', 'etc/sudoers.d', 'var/lib', 'bin'):
            (self.root / name).mkdir(parents=True)
        (self.root / 'accounts').write_text('')
        (self.root / 'etc/sudoers.d/95-arkaos-oobe').write_text('setup-only grant')
        source = Path(os.environ.get(
            'ARKA_OOBE_HELPER_SOURCE',
            Path(__file__).resolve().parents[1] / 'arkaos-oobe-apply',
        )).read_text()
        for prefix in ('/run/', '/etc/', '/var/lib/'):
            source = source.replace(prefix, str(self.root) + prefix)
        self.helper = self.root / 'helper'
        self.helper.write_text(source)
        for name in ('id', 'useradd', 'chpasswd', 'chown', 'systemctl'):
            command = self.root / 'bin' / name
            command.write_text(MOCK_COMMAND)
            command.chmod(0o755)
        self.env = dict(os.environ, OOBE_TEST_ROOT=str(self.root))
        self.env['PATH'] = str(self.root / 'bin') + os.pathsep + '/usr/bin:/bin'

    def run_helper(self, user='alice', autologin='1', password='test-password\n'):
        return subprocess.run(
            ['bash', str(self.helper), user, autologin], input=password,
            text=True, capture_output=True, env=self.env, timeout=5,
        )

    def assert_no_account_changes(self):
        self.assertEqual((self.root / 'accounts').read_text(), '')
        self.assertFalse((self.root / 'password-input').exists())
        self.assertFalse((self.root / 'var/lib/arkaos-firstboot-done').exists())

    def test_rejects_root_and_setup_accounts(self):
        for user in ('root', 'arkasetup', 'daemon'):
            with self.subTest(user=user):
                self.assertNotEqual(self.run_helper(user=user).returncode, 0)
                self.assert_no_account_changes()

    def test_rejects_existing_regular_account(self):
        (self.root / 'accounts').write_text('alice\n')
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assertFalse((self.root / 'password-input').exists())

    def test_rejects_multiline_and_invalid_usernames(self):
        for user in ('alice\nroot', 'alice:root', '-root', 'Alice', 'a' * 32, ''):
            with self.subTest(user=user):
                self.assertNotEqual(self.run_helper(user=user).returncode, 0)
                self.assert_no_account_changes()

    def test_rejects_bad_autologin_flag(self):
        for flag in ('', 'yes', '2', '1\n0'):
            with self.subTest(flag=flag):
                self.assertNotEqual(self.run_helper(autologin=flag).returncode, 0)
                self.assert_no_account_changes()

    def test_rejects_missing_and_short_passwords_before_creation(self):
        for password in ('', '\n', 'short\n'):
            with self.subTest(password=password):
                self.assertNotEqual(self.run_helper(password=password).returncode, 0)
                self.assert_no_account_changes()

    def test_rejects_completed_setup(self):
        (self.root / 'var/lib/arkaos-firstboot-done').touch()
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assertEqual((self.root / 'accounts').read_text(), '')
        self.assertFalse((self.root / 'password-input').exists())

    def test_rejects_concurrent_setup(self):
        with (self.root / 'run/arkaos-oobe.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.assertNotEqual(self.run_helper().returncode, 0)
            self.assert_no_account_changes()

    def test_creates_account_and_revokes_setup_privileges(self):
        result = self.run_helper(password='pass:with\\characters\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'accounts').read_text(), 'alice\n')
        self.assertEqual((self.root / 'password-input').read_text(), 'alice:pass:with\\characters\n')
        self.assertTrue((self.root / 'var/lib/arkaos-firstboot-done').exists())
        self.assertFalse((self.root / 'etc/sudoers.d/95-arkaos-oobe').exists())
        self.assertIn('User=alice', (self.root / 'etc/sddm.conf.d/20-autologin.conf').read_text())
        self.assertNotEqual(self.run_helper().returncode, 0)

    def test_autologin_off_removes_previous_config(self):
        config = self.root / 'etc/sddm.conf.d/20-autologin.conf'
        config.parent.mkdir()
        config.write_text('old config')
        result = self.run_helper(autologin='0')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(config.exists())

    def test_user_creation_failure_does_not_set_password_or_finish(self):
        self.env['OOBE_TEST_FAIL'] = 'useradd'
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assert_no_account_changes()

    def test_failed_password_cannot_reset_account_on_retry(self):
        self.env['OOBE_TEST_FAIL'] = 'chpasswd'
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assertFalse((self.root / 'var/lib/arkaos-firstboot-done').exists())
        self.env.pop('OOBE_TEST_FAIL')
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assertFalse((self.root / 'password-input').exists())

    def test_failed_desktop_configuration_keeps_setup_incomplete(self):
        self.env['OOBE_TEST_FAIL'] = 'systemctl'
        self.assertNotEqual(self.run_helper().returncode, 0)
        self.assertFalse((self.root / 'var/lib/arkaos-firstboot-done').exists())
        self.assertTrue((self.root / 'etc/sudoers.d/95-arkaos-oobe').exists())


if __name__ == '__main__':
    unittest.main()
