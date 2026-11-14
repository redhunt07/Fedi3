#!/usr/bin/env python3
"""Stage an APT repository from .deb files; sign it or explicitly request a preview."""
import argparse
from datetime import datetime, timezone
import gzip
import hashlib
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('packages', nargs='+', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    signing = parser.add_mutually_exclusive_group(required=True)
    signing.add_argument('--signing-key', help='Existing maintainer OpenPGP key ID')
    signing.add_argument('--unsigned-preview', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    if args.unsigned_preview and any((output / 'dists/stable' / name).exists()
                                     for name in ['InRelease', 'Release.gpg']):
        parser.error('Use a separate empty output for an unsigned preview')
    pool = output / 'pool/main/f/fedi3'
    pool.mkdir(parents=True, exist_ok=True)
    entries = []
    for source in args.packages:
        source = source.resolve(strict=True)
        control = subprocess.check_output(['dpkg-deb', '--field', str(source)], text=True).strip()
        if '\nArchitecture: amd64' not in '\n' + control:
            parser.error('Only amd64 packages are supported by this repository recipe')
        target = pool / source.name
        shutil.copy2(source, target)
        data = target.read_bytes()
        fields = [control, f'Filename: {target.relative_to(output)}', f'Size: {len(data)}']
        for field, name in [('MD5sum', 'md5'), ('SHA1', 'sha1'), ('SHA256', 'sha256')]:
            fields.append(f'{field}: {hashlib.new(name, data).hexdigest()}')
        entries.append('\n'.join(fields))
    binary = output / 'dists/stable/main/binary-amd64'
    binary.mkdir(parents=True, exist_ok=True)
    packages = ('\n\n'.join(entries) + '\n\n').encode()
    (binary / 'Packages').write_bytes(packages)
    (binary / 'Packages.gz').write_bytes(gzip.compress(packages, mtime=0))
    release_dir = output / 'dists/stable'
    release = [
        'Origin: Fedi3', 'Label: Fedi3', 'Suite: stable', 'Codename: stable',
        'Architectures: amd64', 'Components: main',
        'Date: ' + datetime.now(timezone.utc).strftime('%a, %d %b %Y %H:%M:%S +0000'),
        'Description: Fedi3 community packages',
    ]
    for label, algorithm in [('MD5Sum', 'md5'), ('SHA256', 'sha256')]:
        release.append(label + ':')
        for path in sorted(binary.iterdir()):
            data = path.read_bytes()
            release.append(f' {hashlib.new(algorithm, data).hexdigest()} {len(data)} {path.relative_to(release_dir)}')
    path = release_dir / 'Release'
    path.write_text('\n'.join(release) + '\n')
    if args.signing_key:
        for options, name in [(['--clearsign'], 'InRelease'), (['--armor', '--detach-sign'], 'Release.gpg')]:
            subprocess.run(['gpg', '--batch', '--yes', '--local-user', args.signing_key,
                            '--output', str(release_dir / name), *options, str(path)], check=True)
        with (output / 'fedi3-archive-keyring.asc').open('wb') as stream:
            subprocess.run(['gpg', '--armor', '--export', args.signing_key], stdout=stream, check=True)
        print('Signed APT repository staged:', output)
    else:
        (output / 'UNSIGNED-PREVIEW.txt').write_text(
            'Preparation only. Sign Release and export the maintainer public key before hosting.\n'
            'Do not ask users to disable signature checking or use trusted=yes.\n')
        print('Unsigned APT preparation staged; signing required before hosting:', output)


if __name__ == '__main__':
    main()
