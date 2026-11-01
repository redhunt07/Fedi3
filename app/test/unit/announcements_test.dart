/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/services/announcements_service.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('AnnouncementsService.parse', () {
    test('AN-01 parses dated entries with bodies', () {
      const raw = '''
# Annunci Fedi3

## 2026-11-01 — Composer avanzato
Testo del primo annuncio.
- punto uno

## 2026-10-30 — OpenAPI
Secondo annuncio.
''';
      final entries = AnnouncementsService.parse(raw);
      expect(entries.length, 2);
      expect(entries[0].date, '2026-11-01');
      expect(entries[0].title, 'Composer avanzato');
      expect(entries[0].body, contains('Testo del primo annuncio.'));
      expect(entries[1].title, 'OpenAPI');
      expect(entries[0].id, isNot(entries[1].id));
    });

    test('AN-02 ignores headings without a date and keeps the newest first',
        () {
      const raw = '''
## 2026-10-01 — Vecchio
corpo
## 2026-11-02 — Nuovo
corpo nuovo
''';
      final entries = AnnouncementsService.parse(raw);
      expect(entries.first.title, 'Nuovo');
      expect(entries.last.title, 'Vecchio');
    });

    test('AN-03 empty input yields no entries', () {
      expect(AnnouncementsService.parse(''), isEmpty);
      expect(AnnouncementsService.parse('# solo titolo'), isEmpty);
    });
  });
}
