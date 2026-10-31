/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  group('l10n (AL-01..02)', () {
    test('AL-01 en/it ARB files have exactly the same message keys', () {
      final en = jsonDecode(File('lib/l10n/app_en.arb').readAsStringSync())
          as Map<String, dynamic>;
      final it = jsonDecode(File('lib/l10n/app_it.arb').readAsStringSync())
          as Map<String, dynamic>;

      Set<String> messages(Map<String, dynamic> raw) =>
          raw.keys.where((k) => !k.startsWith('@')).toSet();

      expect(messages(en), equals(messages(it)));
      expect(messages(en), isNotEmpty);
    });

    test('AL-02 key screens contain no hardcoded user-facing strings', () {
      const files = [
        'lib/ui/screens/onboarding_screen.dart',
        'lib/ui/screens/edit_config_screen.dart',
        'lib/ui/screens/notifications_screen.dart',
        'lib/ui/screens/timelines_screen.dart',
        'lib/ui/screens/media_viewer_screen.dart',
        'lib/ui/widgets/note_card.dart',
      ];
      final pattern =
          RegExp(r"(?:Text|labelText|tooltip|label)\s*[:=]?\s*\(?\s*'([^']*)'");
      final offenders = <String>[];
      for (final path in files) {
        final txt = File(path).readAsStringSync();
        for (final match in pattern.allMatches(txt)) {
          final value = match.group(1)!;
          // Interpolated strings are allowed (dynamic content).
          if (!value.contains(r'$')) {
            offenders.add('$path: $value');
          }
        }
      }
      expect(offenders, isEmpty, reason: 'move these strings to the ARB files');
    });
  });
}
