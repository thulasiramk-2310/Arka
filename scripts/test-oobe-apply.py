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
import json, os, subprocess
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
if name == 'runuser':
    assert sys.argv[1] == '-u' and sys.argv[3] == '--'
    assert sys.argv[2] in (root / 'accounts').read_text().splitlines()
    sys.exit(subprocess.run(sys.argv[4:], check=False).returncode)
'''


class ApplyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='arka-oobe-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ('run', 'etc/sudoers.d', 'var/lib', 'bin'):
            (self.root / name).mkdir(parents=True)
        (self.root / 'accounts').write_text('')
        (self.root / 'etc/arkad').mkdir()
        (self.root / 'etc/arkad/arkad.toml').write_text(
            'check_interval_secs = 60\n\n[dns]\nserver = "9.9.9.9"\n\n[hostname]\nname = "arka"\n')
        (self.root / 'etc/sudoers.d/95-arkaos-oobe').write_text('setup-only grant')
        source = Path(os.environ.get(
            'ARKA_OOBE_HELPER_SOURCE',
            Path(__file__).resolve().parents[1] / 'arkaos-oobe-apply',
        )).read_text()
        for prefix in ('/run/', '/etc/', '/var/lib/', '/home/'):
            source = source.replace(prefix, str(self.root) + prefix)
        self.helper = self.root / 'helper'
        self.helper.write_text(source)
        for name in ('id', 'useradd', 'chpasswd', 'chown', 'systemctl', 'runuser', 'vara', 'git'):
            command = self.root / 'bin' / name
            command.write_text(MOCK_COMMAND)
            command.chmod(0o755)
        self.env = dict(os.environ, OOBE_TEST_ROOT=str(self.root))
        self.env['PATH'] = str(self.root / 'bin') + os.pathsep + '/usr/bin:/bin'

    def run_helper(self, user='alice', autologin='1', password='test-password\n', look=None, dev=None, host=None):
        args = ['bash', str(self.helper), user, autologin]
        if look is not None:
            args.append(look)
        if host is not None or dev is not None:
            args.append(host or 'arka')
        if dev is not None:
            args.extend(dev)
        return subprocess.run(
            args, input=password,
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

    def test_new_look_is_written_through_the_unprivileged_account(self):
        result = self.run_helper(look='light')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'home/alice/.config/arka-look').read_text(), 'light\n')
        import json
        calls = [json.loads(line) for line in (self.root / 'calls').read_text().splitlines()]
        writes = [call for call in calls if call[0] == 'runuser']
        self.assertEqual(len(writes), 1)
        self.assertEqual(writes[0][1:4], ['-u', 'alice', '--'])
        self.assertNotIn('test-password', str(writes))

    def test_light_look_selects_the_light_login_theme(self):
        self.assertEqual(self.run_helper(look='light').returncode, 0)
        theme = self.root / 'etc/sddm.conf.d/30-arka-theme.conf'
        self.assertEqual(theme.read_text(), '[Theme]\nCurrent=arkaos-light\n')

    def test_dark_look_keeps_the_image_login_theme(self):
        self.assertEqual(self.run_helper(look='dark').returncode, 0)
        self.assertFalse((self.root / 'etc/sddm.conf.d/30-arka-theme.conf').exists())

    def test_legacy_caller_defaults_to_dark(self):
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual((self.root / 'home/alice/.config/arka-look').read_text(), 'dark\n')

    def test_invalid_look_is_rejected_before_account_creation(self):
        for look in ['', 'blue', '../dark', 'light\ndark']:
            # An omitted argument defaults to dark; an explicit empty argument
            # follows the helper's current defaulting convention too.
            if not look:
                continue
            self.assertNotEqual(self.run_helper(look=look).returncode, 0)
            self.assert_no_account_changes()

    def test_failed_unprivileged_write_does_not_complete_setup(self):
        self.env['OOBE_TEST_FAIL'] = 'runuser'
        self.assertNotEqual(self.run_helper(look='light').returncode, 0)
        self.assertFalse((self.root / 'var/lib/arkaos-firstboot-done').exists())
        self.assertFalse((self.root / 'home/alice/.config/arka-look').exists())
        self.assertTrue((self.root / 'etc/sudoers.d/95-arkaos-oobe').exists())

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


    def calls(self):
        import json
        path = self.root / 'calls'
        return [json.loads(l) for l in path.read_text().splitlines()] if path.exists() else []

    def test_version_control_is_set_up_as_the_new_account(self):
        result = self.run_helper(look='dark', dev=['vara,git', 'Alice Example', 'alice@example.org'])
        self.assertEqual(result.returncode, 0, result.stderr)
        home = str(self.root / 'home/alice')
        tool_calls = [c for c in self.calls() if c[0] == 'runuser' and c[4:5] == ['env']]
        expected_env = ['env', '--chdir=' + home, 'HOME=' + home, 'XDG_CONFIG_HOME=' + home + '/.config']
        self.assertEqual(tool_calls, [
            ['runuser', '-u', 'alice', '--', *expected_env, 'vara', 'identity', 'set', '--default',
             '--name', 'Alice Example', '--email', 'alice@example.org'],
            ['runuser', '-u', 'alice', '--', *expected_env, 'git', 'config', '--global', 'user.name', 'Alice Example'],
            ['runuser', '-u', 'alice', '--', *expected_env, 'git', 'config', '--global', 'user.email', 'alice@example.org'],
        ])
        self.assertTrue((self.root / 'var/lib/arkaos-firstboot-done').exists())

    def test_without_version_control_no_tool_runs(self):
        result = self.run_helper(look='dark')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse([c for c in self.calls() if c[0] in ('vara', 'git')])

    def test_only_git_when_only_git_is_chosen(self):
        result = self.run_helper(look='light', dev=['git', 'Alice', 'a@b.c'])
        self.assertEqual(result.returncode, 0, result.stderr)
        names = [c[0] for c in self.calls()]
        self.assertIn('git', names)
        self.assertNotIn('vara', names)

    def test_bad_version_control_input_is_rejected_before_account_creation(self):
        for dev in (['svn', 'Alice', 'a@b.c'], ['vara', '', 'a@b.c'], ['vara', 'Alice', 'not-an-email'],
                    ['vara', 'Ali\nce', 'a@b.c'], ['vara', 'Alice', 'a b@c.d'], ['vara;git', 'Alice', 'a@b.c']):
            with self.subTest(dev=dev):
                result = self.run_helper(look='dark', dev=dev)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assert_no_account_changes()

    def test_partial_version_control_arguments_are_rejected(self):
        result = self.run_helper(look='dark', dev=['vara', 'Alice'])
        self.assertEqual(result.returncode, 2)
        self.assert_no_account_changes()

    def test_failed_tool_setup_still_finishes_the_account(self):
        self.env['OOBE_TEST_FAIL'] = 'vara'
        result = self.run_helper(look='dark', dev=['vara', 'Alice', 'a@b.c'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('could not set the VARA identity', result.stderr)
        self.assertTrue((self.root / 'var/lib/arkaos-firstboot-done').exists())

    def test_a_chosen_computer_name_goes_into_arkads_settings(self):
        result = self.run_helper(look='dark', host='ram-pc')
        self.assertEqual(result.returncode, 0, result.stderr)
        toml = (self.root / 'etc/arkad/arkad.toml').read_text()
        self.assertIn('[hostname]\nname = "ram-pc"', toml)
        self.assertIn('server = "9.9.9.9"', toml)

    def test_the_default_computer_name_leaves_arkads_settings_alone(self):
        before = (self.root / 'etc/arkad/arkad.toml').read_text()
        self.assertEqual(self.run_helper(look='dark').returncode, 0)
        self.assertEqual((self.root / 'etc/arkad/arkad.toml').read_text(), before)

    def test_bad_computer_names_are_rejected_before_account_creation(self):
        for host in ('1abc', 'a-', 'Abc', 'my pc', 'a_b', 'b' * 31, 'a;rm', '-x'):
            with self.subTest(host=host):
                result = self.run_helper(look='dark', host=host)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assert_no_account_changes()

if __name__ == '__main__':
    unittest.main()
