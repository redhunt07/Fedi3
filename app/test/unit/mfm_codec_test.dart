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


    test('AF-06 headings, lists, quotes and code blocks produce HTML blocks',
        () {
      final html = MfmCodec.toHtml(
          '# Titolo\n\n> citazione\n\n- uno\n- due\n\n1. primo\n\n```\ncode\n```');
      expect(html, contains('<h1>Titolo</h1>'));
      expect(html, contains('<blockquote>'));
      expect(html, contains('<ul><li>uno</li><li>due</li></ul>'));
      expect(html, contains('<ol><li>primo</li></ol>'));
      expect(html, contains('<pre><code>code</code></pre>'));
    });

    test('AF-07 markdown links and MFM style tokens are converted', () {
      final html = MfmCodec.toHtml(
          '[Fedi3](https://fedi3.test) \$[x2 grande] \$[font.serif testo]');
      expect(html, contains('<a href="https://fedi3.test">Fedi3</a>'));
      expect(html, contains('font-size:2em'));
      expect(html, contains('font-family:serif'));
    });

    test('AF-08 hasMarkers detects block markers', () {
      expect(MfmCodec.hasMarkers('# Titolo'), isTrue);
      expect(MfmCodec.hasMarkers('- voce'), isTrue);
      expect(MfmCodec.hasMarkers('```code```'), isTrue);
      expect(MfmCodec.hasMarkers('solo testo'), isFalse);
    });

    test('AF-05 empty input yields an empty string, not an empty paragraph',
        () {
      expect(MfmCodec.toHtml(''), '');
      expect(MfmCodec.toHtml('   \n  '), '');
    });
  });
}
