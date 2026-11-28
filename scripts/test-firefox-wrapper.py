#!/usr/bin/env python3
"""Check sandbox launch arguments using a fake bwrap and temporary home."""
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
APP_ID = 'org.arka.SandboxedFirefox'
HOST_BUS = f'/run/user/{os.getuid()}/bus'


def host_has_portal_inputs():
    """The wrapper starts the portal proxy only where the proxy program and the session bus exist."""
    try:
        return os.access('/usr/bin/xdg-dbus-proxy', os.X_OK) and stat.S_ISSOCK(os.stat(HOST_BUS).st_mode)
    except OSError:
        return False


PORTAL = host_has_portal_inputs()
needs_portal = unittest.skipUnless(PORTAL, 'xdg-dbus-proxy or the session bus is not available here')


def same_launch(args):
    """The proxy socket name carries the wrapper's process id; drop it to compare two launches."""
    return [re.sub(r'proxy-\d+$', 'proxy-PID', a) for a in args]


class WrapperTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='arka-browser-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / 'home with spaces'
        self.home.mkdir()
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        fake = self.bin / 'bwrap'
        # Stands in for bubblewrap: prints the arguments (the proxy's call goes to the pipe the
        # wrapper reads its ready byte from), logs every call, and saves /.flatpak-info.
        fake.write_text("""#!/usr/bin/python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ['FAKE_LOG'], 'a') as log:
    log.write(json.dumps(args) + '\\n')
if '/.flatpak-info' in args:
    fd = int(args[args.index('/.flatpak-info') - 1])
    with open(os.environ['FAKE_INFO'], 'wb') as info:
        info.write(os.read(fd, 4096))
if 'xdg-dbus-proxy' in ' '.join(args) and os.environ.get('FAKE_PROXY_FAILS'):
    sys.exit(1)
print(json.dumps(args))
""")
        fake.chmod(0o755)
        self.wrapper = Path(os.environ.get('ARKA_BROWSER_WRAPPER_SOURCE',
            Path(__file__).resolve().parents[1] / 'arkaos-firefox'))
        self.env = dict(os.environ, HOME=str(self.home), WAYLAND_DISPLAY='wayland-0',
                        PATH=str(self.bin) + ':/usr/bin:/bin', ARKA_TEST_SECRET='synthetic-secret',
                        FAKE_LOG=str(self.root / 'calls.jsonl'), FAKE_INFO=str(self.root / 'flatpak-info'))

    def run_wrapper(self, *args):
        return subprocess.run(['bash', str(self.wrapper), *args], env=self.env,
                              capture_output=True, text=True, timeout=5)

    @staticmethod
    def pairs(args, flag):
        return [(args[i + 1], args[i + 2]) for i, arg in enumerate(args) if arg == flag]

    def binds(self, args):
        """The writable binds other than the filtered portal socket and the document folder."""
        return [(src, dst) for src, dst in self.pairs(args, '--bind') if dst not in (HOST_BUS, f"/run/user/{os.getuid()}/doc")]

    def calls(self):
        """Every bubblewrap call the wrapper made: the proxy's first (if any), then the browser's."""
        return [json.loads(line) for line in (self.root / 'calls.jsonl').read_text().splitlines()]

    def test_environment_is_allowlisted_and_capabilities_are_dropped(self):
        result = self.run_wrapper('https://example.invalid/')
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(result.stdout)
        self.assertIn('--clearenv', args)
        self.assertEqual(args[args.index('--cap-drop') + 1], 'ALL')
        names = {args[i + 1] for i, value in enumerate(args) if value == '--setenv'}
        expected = {'PATH', 'HOME', 'LANG', 'LC_ALL', 'XDG_RUNTIME_DIR',
                    'WAYLAND_DISPLAY', 'MOZ_ENABLE_WAYLAND', 'PULSE_SERVER'}
        if PORTAL:
            expected.add('DBUS_SESSION_BUS_ADDRESS')
        self.assertEqual(names, expected)
        self.assertNotIn('synthetic-secret', args)
        self.assertIn('--new-session', args)

    def test_audio_sockets_and_server_are_explicit_without_exposing_runtime_directory(self):
        self.env['PULSE_SERVER'] = 'tcp:untrusted.invalid'
        args = json.loads(self.run_wrapper().stdout)
        runtime = f'/run/user/{os.getuid()}'
        binds = self.pairs(args, '--bind-try')
        for socket in [f'{runtime}/pulse/native', f'{runtime}/pipewire-0']:
            self.assertIn((socket, socket), binds)
        self.assertNotIn((runtime, runtime), binds)
        self.assertNotIn((f'{runtime}/pulse', f'{runtime}/pulse'), binds)
        pos = args.index('PULSE_SERVER')
        self.assertEqual(args[pos + 1], f'unix:{runtime}/pulse/native')
        self.assertNotIn('tcp:untrusted.invalid', args)

    def test_downloads_symlink_cannot_expose_the_whole_home(self):
        (self.home / 'Downloads').symlink_to(self.home, target_is_directory=True)
        result = self.run_wrapper()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('symlink', result.stderr)
        self.assertEqual(result.stdout, '')

    def test_downloads_symlink_to_another_directory_is_rejected(self):
        outside = self.root / 'outside'
        outside.mkdir()
        (self.home / 'Downloads').symlink_to(outside, target_is_directory=True)
        self.assertNotEqual(self.run_wrapper().returncode, 0)

    def test_socket_path_cannot_request_arbitrary_host_binds(self):
        for name in ['../../other', '/tmp/socket', '..', '']:
            if not name:
                continue  # An empty value intentionally selects wayland-0.
            self.env['WAYLAND_DISPLAY'] = name
            self.assertNotEqual(self.run_wrapper().returncode, 0, name)

    def test_shell_and_browser_use_the_same_boundary(self):
        normal = json.loads(self.run_wrapper().stdout)
        shell = json.loads(self.run_wrapper('--shell', '-c', 'true').stdout)
        self.assertEqual(same_launch(normal[:-1]), same_launch(shell[:-3]))
        self.assertEqual(shell[-3:], ['/bin/bash', '-c', 'true'])

    def test_browser_arguments_are_preserved_without_shell_evaluation(self):
        url = 'https://example.invalid/;touch /tmp/never-run'
        args = json.loads(self.run_wrapper(url).stdout)
        self.assertEqual(args[-1], url)
        self.assertEqual(self.binds(args), [(str(self.home / 'Downloads'),) * 2])

    def test_host_buses_are_never_bound_and_the_namespaces_stay(self):
        runtime = f'/run/user/{os.getuid()}'
        args = json.loads(self.run_wrapper().stdout)
        joined = '\n'.join(args)
        self.assertNotIn('/run/dbus', joined)
        self.assertNotIn('/var/run', joined)
        self.assertNotIn('/.flatpak-info', joined)
        for flag in ['--bind', '--bind-try', '--ro-bind', '--ro-bind-try']:
            self.assertNotIn(HOST_BUS, [src for src, _ in self.pairs(args, flag)])
        expected = [f'{runtime}/wayland-0', f'{runtime}/pipewire-0', f'{runtime}/pulse/native']
        self.assertEqual(sorted(dst for _, dst in self.pairs(args, '--bind-try')), sorted(expected))
        self.assertEqual(sorted(a for a in args if a.startswith('--unshare') or a == '--share-net'),
                         ['--unshare-ipc', '--unshare-pid', '--unshare-uts'])
        self.assertIn('--die-with-parent', args)
        etc = sorted(dst for _, dst in self.pairs(args, '--ro-bind-try'))
        self.assertEqual(etc, sorted('/etc/' + n for n in [
            'resolv.conf', 'hosts', 'nsswitch.conf', 'ssl', 'pki', 'fonts', 'localtime',
            'ld.so.cache', 'ld.so.conf.d', 'alternatives']))
        tmpfs = [args[i + 1] for i, arg in enumerate(args) if arg == '--tmpfs']
        self.assertEqual(tmpfs, ['/etc', '/tmp', '/run', '/home', '/root'])

    @needs_portal
    def test_the_browser_gets_only_the_filtered_socket_and_the_document_folder(self):
        args = json.loads(self.run_wrapper().stdout)
        runtime = f'/run/user/{os.getuid()}'
        socks = [src for src, dst in self.pairs(args, '--bind') if dst == HOST_BUS]
        self.assertEqual(len(socks), 1)
        self.assertRegex(socks[0], rf'^{runtime}/arkaos-firefox/proxy-\d+$')
        self.assertEqual(args[args.index('DBUS_SESSION_BUS_ADDRESS') + 1], f'unix:path={HOST_BUS}')
        self.assertIn((f'{runtime}/doc/by-app/{APP_ID}', f'{runtime}/doc'), self.pairs(args, '--bind'))
        # The identity file belongs to the proxy alone.
        self.assertNotIn('/.flatpak-info', args)
        self.assertNotIn('--ro-bind-data', args)

    @needs_portal
    def test_the_proxy_runs_in_its_own_bubblewrap_as_the_named_app_with_a_narrow_filter(self):
        self.run_wrapper()
        proxy, browser = self.calls()
        self.assertEqual(browser[-1], '/usr/bin/firefox-unwrapped')
        self.assertEqual(self.pairs(proxy, '--ro-bind-data')[0][1], '/.flatpak-info')
        self.assertEqual((self.root / 'flatpak-info').read_text().splitlines()[:2],
                         ['[Application]', f'name={APP_ID}'])
        self.assertIn('--die-with-parent', proxy)
        self.assertEqual(proxy[proxy.index('--cap-drop') + 1], 'ALL')
        self.assertIn(('/usr', '/usr'), self.pairs(proxy, '--ro-bind'))
        # Nothing of the user's home or Downloads is in the proxy's view.
        self.assertNotIn(str(self.home), ' '.join(proxy))
        rest = proxy[proxy.index('/usr/bin/xdg-dbus-proxy'):]
        self.assertEqual(rest[1], '--fd=1')
        self.assertEqual(rest[2], f'unix:path={HOST_BUS}')
        self.assertRegex(rest[3], r'/arkaos-firefox/proxy-\d+$')
        portal = 'org.freedesktop.portal.Desktop'
        path = '@/org/freedesktop/portal/desktop'
        self.assertEqual(sorted(rest[4:]), sorted([
            '--filter',
            f'--call={portal}=org.freedesktop.portal.FileChooser.*{path}',
            f'--call={portal}=org.freedesktop.DBus.Properties.Get{path}',
            f'--broadcast={portal}=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*',
            '--own=org.mozilla.firefox.*']))

    def test_the_proxy_rules_in_the_script_are_exactly_the_known_list(self):
        text = self.wrapper.read_text()
        block = text[text.index('local rules=(') + len('local rules=('):]
        block = block[:block.index('\n    )\n')]
        lines = [line.strip().strip("'") for line in block.splitlines() if line.strip()]
        portal = 'org.freedesktop.portal.Desktop'
        path = '@/org/freedesktop/portal/desktop'
        self.assertEqual(lines, [
            '--filter',
            f'--call={portal}=org.freedesktop.portal.FileChooser.*{path}',
            f'--call={portal}=org.freedesktop.DBus.Properties.Get{path}',
            f'--broadcast={portal}=org.freedesktop.portal.Request.*@/org/freedesktop/portal/desktop/request/*'])
        self.assertNotIn('--see', text)
        self.assertNotIn('--talk', text)
        self.assertEqual(re.findall(r"--own=\S+", text), ["--own=org.mozilla.firefox.*')"])

    @needs_portal
    def test_without_a_working_proxy_the_browser_starts_with_no_bus(self):
        self.env['FAKE_PROXY_FAILS'] = '1'
        result = self.run_wrapper('https://example.invalid/')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('using the browser file dialog', result.stderr)
        args = json.loads(result.stdout)
        self.assertNotIn('DBUS_SESSION_BUS_ADDRESS', args)
        self.assertNotIn(HOST_BUS, args)
        self.assertEqual(args[-2:], ['/usr/bin/firefox-unwrapped', 'https://example.invalid/'])
        self.assertEqual(list((Path(HOST_BUS).parent / 'arkaos-firefox').glob('proxy-*')), [])

    def test_policies_send_the_file_dialog_through_the_portal(self):
        policies = json.loads((REPO / 'firefox-policies.json').read_text())['policies']
        self.assertEqual(policies['Preferences']['widget.use-xdg-desktop-portal.file-picker'],
                         {'Value': 1, 'Status': 'default'})


if __name__ == '__main__':
    unittest.main()
