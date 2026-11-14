#!/usr/bin/env python3
"""Verify the staged APT repository against a real Debian package and GPG key."""
import gzip
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(all(shutil.which(c) for c in ['dpkg-deb', 'gpg', 'gpgv']),
                     'requires Debian packaging and GnuPG tools')
class AptRepository(unittest.TestCase):
    def test_signed_repository_matches_package_and_preserves_signed_preview(self):
        with tempfile.TemporaryDirectory(prefix='fedi3-apt-test-') as directory:
            root = Path(directory)
            control = root / 'package/DEBIAN/control'
            control.parent.mkdir(parents=True)
            control.write_text('Package: fedi3-fixture\nVersion: 0.0.0\nArchitecture: amd64\n'
                               'Maintainer: Fixture <fixture@example.invalid>\n'
                               'Description: repository test fixture\n')
            package = root / 'fixture.deb'
            subprocess.run(['dpkg-deb', '--root-owner-group', '--build',
                            str(control.parent.parent), str(package)], check=True,
                           stdout=subprocess.DEVNULL)
            home = root / 'gnupg'
            home.mkdir(mode=0o700)
            env = dict(os.environ, GNUPGHOME=str(home))
            subprocess.run(['gpg', '--batch', '--pinentry-mode', 'loopback',
                            '--passphrase', '', '--quick-generate-key',
                            'Fedi3 fixture <fixture@example.invalid>', 'ed25519', 'sign', '1d'],
                           env=env, check=True, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
            output = root / 'repo'
            args = ['python3', str(ROOT / 'scripts/build_apt_repo.py'),
                    '--output', str(output)]
            subprocess.run([*args, '--signing-key', 'fixture@example.invalid', str(package)],
                           env=env, check=True, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
            binary = output / 'dists/stable/main/binary-amd64'
            index = (binary / 'Packages').read_bytes()
            self.assertEqual(gzip.decompress((binary / 'Packages.gz').read_bytes()), index)
            self.assertIn(hashlib.sha256(package.read_bytes()).hexdigest().encode(), index)
            self.assertEqual((output / 'pool/main/f/fedi3/fixture.deb').read_bytes(),
                             package.read_bytes())
            keyring = root / 'trusted.gpg'
            keyring.write_bytes(subprocess.check_output(['gpg', '--export'], env=env))
            subprocess.run(['gpgv', '--keyring', str(keyring),
                            str(output / 'dists/stable/InRelease')], env=env,
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            release = (output / 'dists/stable/Release').read_bytes()
            result = subprocess.run([*args, '--unsigned-preview', str(package)], env=env,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual((output / 'dists/stable/Release').read_bytes(), release)


if __name__ == '__main__':
    unittest.main()
