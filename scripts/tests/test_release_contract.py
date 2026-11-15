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
    def test_project_license_is_consistent(self):
        for manifest in (ROOT / 'crates').glob('*/Cargo.toml'):
            self.assertIn('license = "AGPL-3.0-only"', manifest.read_text())
        for name in ['README.md', 'README.it.md', 'CONTRIBUTING.md',
                     'FORK_POLICY.md', 'TRADEMARK_AND_NAME.md', 'NOTICE']:
            text = (ROOT / name).read_text()
            self.assertIn('AGPL-3.0-only', text)
            self.assertNotIn('AGPL-3.0-or-later', text)

    def test_validation_branches_build_every_package_without_publishing(self):
        workflow = yaml.safe_load((ROOT / '.github/workflows/ci.yml').read_text())
        # PyYAML's YAML 1.1 loader interprets the unquoted "on" key as True.
        triggers = workflow.get('on', workflow.get(True))
        self.assertIn('codex/verify-release/*', triggers['push']['branches'])
        for job in ['appimage', 'windows-release', 'android-release', 'linux-smoke']:
            self.assertIn('refs/heads/codex/verify-release/', workflow['jobs'][job]['if'])
        self.assertNotIn('refs/heads/codex/verify-release/', workflow['jobs']['publish']['if'])
        audit = next(s for s in workflow['jobs']['rust']['steps']
                     if s.get('name') == 'cargo audit (required for releases)')
        self.assertIn("!startsWith(github.ref, 'refs/heads/codex/verify-release/')",
                      audit['continue-on-error'])

    def test_release_notes_include_both_languages(self):
        version = re.search(r'^version: ([^+]+)',
                            (ROOT / 'app/pubspec.yaml').read_text(), re.M).group(1)
        notes = (ROOT / f'docs/releases/v{version}.md').read_text()
        self.assertIn('## Italiano', notes)
        self.assertIn('## English', notes)
        workflow = yaml.safe_load((ROOT / '.github/workflows/ci.yml').read_text())
        publish = str(workflow['jobs']['publish'])
        self.assertIn('--notes-file', publish)
        self.assertNotIn('--generate-notes', publish)

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
        self.assertIn('linux-smoke', publish['needs'])
        self.assertEqual(workflow['jobs']['linux-smoke']['strategy']['matrix']['os'],
                         ['ubuntu-22.04', 'ubuntu-24.04'])
        self.assertIn('Release request is not current main', str(publish))
        self.assertIn('--draft=false --latest', str(publish))
        self.assertIn('Fedi3-linux-x86_64.AppImage', str(workflow['jobs']['appimage']))
        self.assertIn('Fedi3-windows-x64.zip', str(workflow['jobs']['windows-release']))


if __name__ == '__main__':
    unittest.main()
