#!/usr/bin/env python3
"""Reproduce a newer host GLib dependency shadowed by an older bundled ABI."""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
POLICY = ROOT / 'scripts/lib/appimage_host_libs.sh'


def uses_host(name):
    return subprocess.run(
        ['bash', '-c', 'source "$1"; appimage_uses_host_library "$2"',
         'bash', str(POLICY), name], check=False).returncode == 0


class HostLibraries(unittest.TestCase):
    def test_host_desktop_dependencies_and_application_libraries(self):
        for name in ['libX11.so.6', 'libX11-xcb.so.1', 'libxcb.so.1',
                     'libmount.so.1', 'libblkid.so.1', 'libsystemd.so.0',
                     'libudev.so.1', 'libpcre2-8.so.0', 'libffi.so.8',
                     'libgthread-2.0.so.0', 'libfontconfig.so.1',
                     'libfreetype.so.6', 'libharfbuzz.so.0', 'libexpat.so.1',
                     'libz.so.1']:
            with self.subTest(name=name):
                self.assertTrue(uses_host(name))
        for name in ['libfedi3_core.so', 'libflutter_linux_gtk.so',
                     'libmpv.so.1', 'libwebkit2gtk-4.1.so.0', 'libpcre.so.3']:
            with self.subTest(name=name):
                self.assertFalse(uses_host(name))

    def test_newer_host_mount_abi_is_not_shadowed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            host, bundle = root / 'host', root / 'bundle'
            host.mkdir()
            bundle.mkdir()
            source = root / 'mount.c'
            source.write_text('int mount_version(void) { return 1; }\n')
            for directory, version in [(host, 'MOUNT_2_40'), (bundle, 'MOUNT_2_35')]:
                mapping = directory / 'version.map'
                mapping.write_text(f'{version} {{ global: mount_version; }};\n')
                subprocess.run(['cc', '-shared', '-fPIC', str(source),
                                '-Wl,-soname,libmount.so.1',
                                f'-Wl,--version-script={mapping}',
                                '-o', str(directory / 'libmount.so.1')], check=True)
            client = root / 'client.c'
            client.write_text('extern int mount_version(void);\n'
                              'int main(void) { return mount_version() != 1; }\n')
            executable = root / 'client'
            subprocess.run(['cc', str(client), f'-L{host}', '-l:libmount.so.1',
                            f'-Wl,-rpath,{host}', '-o', str(executable)], check=True)
            env = dict(os.environ, LD_LIBRARY_PATH=str(bundle))
            broken = subprocess.run([str(executable)], env=env, capture_output=True)
            self.assertNotEqual(broken.returncode, 0)
            self.assertIn(b'MOUNT_2_40', broken.stderr)
            self.assertTrue(uses_host('libmount.so.1'))
            (bundle / 'libmount.so.1').unlink()
            subprocess.run([str(executable)], env=env, check=True)


if __name__ == '__main__':
    unittest.main()
