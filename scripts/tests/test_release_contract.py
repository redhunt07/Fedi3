#!/usr/bin/env python3
"""Validate release tags, package names and the read-only default CI token."""
import os
from pathlib import Path
import re
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


def check_tag(tag: str) -> None:
    text = (ROOT / 'app/pubspec.yaml').read_text()
    version = re.search(r'^version: (\d+\.\d+\.\d+)\+\d+$', text, re.M)
    if not version or tag != f'v{version.group(1)}':
        raise ValueError('Release tag must match app/pubspec.yaml (vX.Y.Z)')


class ReleaseContract(unittest.TestCase):
    def test_tag_matches_version(self):
        tag = os.environ.get('FEDI3_RELEASE_TAG')
        if tag:
            check_tag(tag)
        with self.assertRaises(ValueError):
            check_tag('v9999.0.0')

    def test_release_graph_has_no_retired_rsa_or_dtls_provider(self):
        packages = (ROOT / 'Cargo.lock').read_text().split('[[package]]')
        for package in packages:
            name = re.search(r'^name = "([^"]+)"$', package, re.M)
            version = re.search(r'^version = "([^"]+)"$', package, re.M)
            if not name or not version:
                continue
            self.assertNotIn(name.group(1), {'rsa', 'webrtc-dtls'})
            if name.group(1) == 'crypto-bigint':
                number = tuple(map(int, version.group(1).split('.')))
                self.assertGreaterEqual(number, (0, 7, 6))

    def test_ci_permissions_and_release_assets(self):
        workflow = yaml.safe_load((ROOT / '.github/workflows/ci.yml').read_text())
        self.assertEqual(workflow['permissions'], {'contents': 'read'})
        self.assertEqual(workflow['jobs']['publish']['permissions'], {'contents': 'write'})
        self.assertIn('checksums.txt', str(workflow['jobs']['publish']))
        audit = next(s for s in workflow['jobs']['rust']['steps']
                     if s.get('name') == 'cargo audit (required for releases)')
        self.assertIn("!startsWith(github.ref, 'refs/tags/')", audit['continue-on-error'])
        self.assertIn("!startsWith(github.ref, 'refs/heads/codex/release/')", audit['continue-on-error'])
        publish = workflow['jobs']['publish']
        self.assertIn('Release request is not current main', str(publish))
        self.assertIn('--draft=false --latest', str(publish))
        self.assertIn('Fedi3-linux-x86_64.AppImage', str(workflow['jobs']['appimage']))
        self.assertIn('Fedi3-windows-x64.zip', str(workflow['jobs']['windows-release']))


if __name__ == '__main__':
    unittest.main()
