/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/utils/mfm_codec.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('MfmCodec (AF-01..05)', () {
    test('AF-01 escapes <, &, double and single quotes', () {
      final html = MfmCodec.toHtml('1 < 2 & 3 "quoted" \'single\'');
      expect(html, contains('&lt;'));
      expect(html, contains('&amp;'));
      expect(html, contains('&quot;'));
      expect(html, contains('&#39;'));
      expect(html, isNot(contains('<b>')));
    });

    test('AF-02 text without MFM markers is still escaped HTML', () {
      final html = MfmCodec.toHtml('plain <script>alert(1)</script> & text');
      expect(html, startsWith('<p>'));
      expect(html, endsWith('</p>'));
      expect(html, contains('&lt;script&gt;'));
      expect(html, contains('&amp;'));
      expect(html, isNot(contains('<script>')));
    });

    test('AF-03 newlines become <br>', () {
      final html = MfmCodec.toHtml('line one\nline two');
      expect(html, contains('<br>'));
      expect(html, contains('line one'));
      expect(html, contains('line two'));
    });

    test('AF-04 MFM markers produce expected HTML', () {
      final html = MfmCodec.toHtml('**bold** ~~gone~~ `code` :blob:');
      expect(html, contains('<strong>bold</strong>'));
      expect(html, contains('<del>gone</del>'));
      expect(html, contains('<code>code</code>'));
    });

    test('AF-05 empty input yields an empty string, not an empty paragraph',
        () {
      expect(MfmCodec.toHtml(''), '');
      expect(MfmCodec.toHtml('   \n  '), '');
    });
  });
}
