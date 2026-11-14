#!/usr/bin/env python3
"""Select manual pairing and remove ML Kit before resolving F-Droid dependencies."""
from pathlib import Path
import argparse
parser = argparse.ArgumentParser()
parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
root = parser.parse_args().root
pubspec = root / 'app/pubspec.yaml'
text = pubspec.read_text()
text = '\n'.join(line for line in text.split('\n') if not line.lstrip().startswith('mobile_scanner:'))
pubspec.write_text(text)
(root / 'app/lib/ui/widgets/pairing_scanner.dart').write_bytes((root / 'packaging/fdroid/pairing_scanner.dart.in').read_bytes())
