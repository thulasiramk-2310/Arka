#!/usr/bin/env python3
"""Check sandbox launch arguments using a fake bwrap and temporary home."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


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
        fake.write_text('#!/usr/bin/python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n')
        fake.chmod(0o755)
        self.wrapper = Path(os.environ.get('ARKA_BROWSER_WRAPPER_SOURCE',
            Path(__file__).resolve().parents[1] / 'arkaos-firefox'))
        self.env = dict(os.environ, HOME=str(self.home), WAYLAND_DISPLAY='wayland-0',
                        PATH=str(self.bin) + ':/usr/bin:/bin', ARKA_TEST_SECRET='synthetic-secret')

    def run_wrapper(self, *args):
        return subprocess.run(['bash', str(self.wrapper), *args], env=self.env,
                              capture_output=True, text=True, timeout=5)

    def test_environment_is_allowlisted_and_capabilities_are_dropped(self):
        result = self.run_wrapper('https://example.invalid/')
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(result.stdout)
        self.assertIn('--clearenv', args)
        self.assertEqual(args[args.index('--cap-drop') + 1], 'ALL')
        names = {args[i + 1] for i, value in enumerate(args) if value == '--setenv'}
        self.assertEqual(names, {'PATH', 'HOME', 'LANG', 'LC_ALL', 'XDG_RUNTIME_DIR',
                                 'WAYLAND_DISPLAY', 'MOZ_ENABLE_WAYLAND', 'PULSE_SERVER'})
        self.assertNotIn('synthetic-secret', args)
        self.assertIn('--new-session', args)

    def test_audio_sockets_and_server_are_explicit_without_exposing_runtime_directory(self):
        self.env['PULSE_SERVER'] = 'tcp:untrusted.invalid'
        args = json.loads(self.run_wrapper().stdout)
        runtime = f'/run/user/{os.getuid()}'
        binds = [(args[i + 1], args[i + 2]) for i, arg in enumerate(args) if arg == '--bind-try']
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
        self.assertEqual(normal[:-1], shell[:-3])
        self.assertEqual(shell[-3:], ['/bin/bash', '-c', 'true'])

    def test_browser_arguments_are_preserved_without_shell_evaluation(self):
        url = 'https://example.invalid/;touch /tmp/never-run'
        args = json.loads(self.run_wrapper(url).stdout)
        self.assertEqual(args[-1], url)
        self.assertEqual(args[args.index('--bind') + 1:args.index('--bind') + 3],
                         [str(self.home / 'Downloads')] * 2)


if __name__ == '__main__':
    unittest.main()
