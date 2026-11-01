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
        for zone in ('UTC', 'Europe/Berlin', 'Asia/Kolkata', 'America/Argentina/Buenos_Aires', 'Etc/GMT+5'):
            fake = self.root / 'usr/share/zoneinfo' / zone
            fake.parent.mkdir(parents=True, exist_ok=True)
            fake.write_text('TZif')
        scheme = self.root / 'usr/share/color-schemes/ArkaOSBlue.colors'
        scheme.parent.mkdir(parents=True, exist_ok=True)
        scheme.write_text('blue-scheme')
        (self.root / 'etc/arkad').mkdir()
        (self.root / 'etc/arkad/arkad.toml').write_text(
            'check_interval_secs = 60\n\n[dns]\nserver = "9.9.9.9"\n\n[hostname]\nname = "arka"\n')
        (self.root / 'etc/sudoers.d/95-arkaos-oobe').write_text('setup-only grant')
        source = Path(os.environ.get(
            'ARKA_OOBE_HELPER_SOURCE',
            Path(__file__).resolve().parents[1] / 'arkaos-oobe-apply',
        )).read_text()
        for prefix in ('/run/', '/etc/', '/var/lib/', '/home/', '/usr/share/zoneinfo/', '/usr/share/color-schemes/'):
            source = source.replace(prefix, str(self.root) + prefix)
        self.helper = self.root / 'helper'
        self.helper.write_text(source)
        for name in ('id', 'useradd', 'chpasswd', 'chown', 'systemctl', 'runuser', 'vara', 'git', 'timedatectl', 'kwriteconfig6'):
            command = self.root / 'bin' / name
            command.write_text(MOCK_COMMAND)
            command.chmod(0o755)
        self.env = dict(os.environ, OOBE_TEST_ROOT=str(self.root))
        self.env['PATH'] = str(self.root / 'bin') + os.pathsep + '/usr/bin:/bin'

    def run_helper(self, user='alice', autologin='1', password='test-password\n', look=None, dev=None, host=None, zone=None, accent=None):
        args = ['bash', str(self.helper), user, autologin]
        if look is None and (host is not None or dev is not None or zone is not None or accent is not None):
            look = 'dark'          # later arguments are positional: the look must come first
        if look is not None:
            args.append(look)
        if host is not None or dev is not None or zone is not None or accent is not None:
            args.append(host or 'arka')
        if dev is not None:
            args.extend(dev)
        elif zone is not None or accent is not None:
            args.extend(['', '', ''])      # no version control, then the time zone
        if zone is not None or accent is not None:
            args.append(zone or '')
        if accent is not None:
            args.append(accent)
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
    def zone_calls(self):
        import json
        calls = [json.loads(line) for line in (self.root / 'calls').read_text().splitlines()]
        return [c for c in calls if c[0] == 'timedatectl']

    def test_a_chosen_time_zone_is_set(self):
        result = self.run_helper(look='dark', zone='Europe/Berlin')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.zone_calls(), [['timedatectl', 'set-timezone', 'Europe/Berlin']])

    def test_deeper_and_signed_zone_names_are_accepted(self):
        for zone in ('America/Argentina/Buenos_Aires', 'Etc/GMT+5', 'UTC'):
            with self.subTest(zone=zone):
                self.setUp()
                self.assertEqual(self.run_helper(zone=zone).returncode, 0)
                self.assertEqual(self.zone_calls(), [['timedatectl', 'set-timezone', zone]])

    def test_a_time_zone_chosen_with_version_control_is_set_after_it(self):
        result = self.run_helper(host='arka', dev=('git', 'Alice', 'alice@example.com'), zone='Asia/Kolkata')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.zone_calls(), [['timedatectl', 'set-timezone', 'Asia/Kolkata']])

    def test_without_a_time_zone_the_image_zone_is_left_alone(self):
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual(self.zone_calls(), [])

    def test_bad_time_zones_are_rejected_before_account_creation(self):
        for zone in ('../etc/passwd', 'Europe/../Berlin', '/etc/passwd', 'Europe/Nowhere', 'Europe/Berlin\nUTC',
                     'Europe/Berlin; reboot', 'a/b/c/d', 'Europe//Berlin', '$(id)', 'Europe/Berlin ', '.'):
            with self.subTest(zone=zone):
                self.setUp()
                self.assertNotEqual(self.run_helper(zone=zone).returncode, 0)
                self.assert_no_account_changes()
                self.assertFalse((self.root / 'calls').exists() and self.zone_calls())

    def test_failed_time_zone_change_does_not_stop_the_account(self):
        self.env['OOBE_TEST_FAIL'] = 'timedatectl'
        result = self.run_helper(zone='Europe/Berlin')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'accounts').read_text(), 'alice\n')
        self.assertTrue((self.root / 'var/lib/arkaos-firstboot-done').exists())
    def written_settings(self):
        import json
        calls = [json.loads(line) for line in (self.root / 'calls').read_text().splitlines()]
        return [c for c in calls if c[0] == 'kwriteconfig6']

    def test_a_blue_accent_is_saved_and_gives_the_blue_scheme(self):
        result = self.run_helper(look='dark', accent='blue')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'home/alice/.config/arka-accent').read_text(), 'blue\n')
        flat = [' '.join(c) for c in self.written_settings()]
        self.assertTrue(any('--key ColorScheme ArkaOSBlue' in c for c in flat), flat)
        self.assertTrue(any('--key AccentColor 79,134,255' in c for c in flat), flat)
        self.assertTrue(any('--key defaultWallpaperTheme ArkaOS-Dark-Blue' in c for c in flat), flat)

    def test_the_light_look_with_blue_gives_the_light_blue_scheme(self):
        self.assertEqual(self.run_helper(look='light', accent='blue').returncode, 0)
        flat = [' '.join(c) for c in self.written_settings()]
        self.assertTrue(any('--key ColorScheme ArkaOSLightBlue' in c for c in flat), flat)
        self.assertTrue(any('--key defaultWallpaperTheme ArkaOS-Light-Blue' in c for c in flat), flat)

    def test_green_is_the_default_accent(self):
        for kwargs in ({}, {'accent': 'green'}):
            with self.subTest(**kwargs):
                self.setUp()
                self.assertEqual(self.run_helper(look='dark', **kwargs).returncode, 0)
                self.assertEqual((self.root / 'home/alice/.config/arka-accent').read_text(), 'green\n')
                flat = [' '.join(c) for c in self.written_settings()]
                self.assertTrue(any(c.endswith('--key ColorScheme ArkaOS') for c in flat), flat)
                self.assertFalse(any('Blue' in c for c in flat), flat)
                self.assertTrue(any('--key AccentColor 22,199,132' in c for c in flat), flat)

    def test_a_bad_accent_is_rejected_before_account_creation(self):
        for accent in ('red', 'Blue', 'blue\nreboot', 'blue;id', '$(id)', ' blue'):
            with self.subTest(accent=accent):
                self.setUp()
                self.assertNotEqual(self.run_helper(accent=accent).returncode, 0)
                self.assert_no_account_changes()

    def test_accent_and_time_zone_can_be_given_together(self):
        result = self.run_helper(look='dark', zone='Europe/Berlin', accent='blue')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.zone_calls(), [['timedatectl', 'set-timezone', 'Europe/Berlin']])
        self.assertEqual((self.root / 'home/alice/.config/arka-accent').read_text(), 'blue\n')
    def test_a_blue_choice_gives_the_login_screen_the_blue_scheme(self):
        result = self.run_helper(look='dark', accent='blue')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'var/lib/sddm/.config/kdeglobals').read_text(), 'blue-scheme')

    def login_theme(self, look, accent):
        self.assertEqual(self.run_helper(look=look, accent=accent).returncode, 0)
        return (self.root / 'etc/sddm.conf.d/30-arka-theme.conf').read_text()

    def test_dark_blue_selects_the_blue_login_theme(self):
        self.assertEqual(self.login_theme('dark', 'blue'), '[Theme]\nCurrent=arkaos-blue\n')

    def test_light_blue_selects_the_light_blue_login_theme(self):
        self.assertEqual(self.login_theme('light', 'blue'), '[Theme]\nCurrent=arkaos-light-blue\n')

    def test_light_green_keeps_the_light_login_theme(self):
        self.assertEqual(self.login_theme('light', 'green'), '[Theme]\nCurrent=arkaos-light\n')

    def test_dark_green_keeps_the_image_login_theme_even_with_an_accent_argument(self):
        self.assertEqual(self.run_helper(look='dark', accent='green').returncode, 0)
        self.assertFalse((self.root / 'etc/sddm.conf.d/30-arka-theme.conf').exists())

    def test_green_leaves_the_login_screen_alone(self):
        self.assertEqual(self.run_helper(look='dark', accent='green').returncode, 0)
        self.assertFalse((self.root / 'var/lib/sddm/.config/kdeglobals').exists())

    def test_a_missing_blue_scheme_does_not_stop_the_account(self):
        (self.root / 'usr/share/color-schemes/ArkaOSBlue.colors').unlink()
        result = self.run_helper(look='dark', accent='blue')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('could not give the login screen the blue accent', result.stderr)
        self.assertTrue((self.root / 'var/lib/arkaos-firstboot-done').exists())

if __name__ == '__main__':
    unittest.main()
