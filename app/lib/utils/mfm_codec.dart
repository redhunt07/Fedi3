/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

/// Converts a Markdown/MFM subset into sanitized HTML for ActivityPub.
/// Supported blocks: headings, quotes, bullet/numbered lists, fenced code,
/// dividers and paragraphs. Inline: bold, italic, strike, code, links and a
/// small set of Misskey MFM style tokens.
class MfmCodec {
  static String toHtml(String input) {
    final raw = input.replaceAll('\r\n', '\n').replaceAll('\r', '\n').trim();
    if (raw.isEmpty) return '';

    final lines = raw.split('\n');
    final out = <String>[];
    var i = 0;
    while (i < lines.length) {
      final line = lines[i];
      if (line.trim().isEmpty) {
        i++;
        continue;
      }
      if (line.trimLeft().startsWith('```')) {
        i++;
        final code = <String>[];
        while (i < lines.length && !lines[i].trimLeft().startsWith('```')) {
          code.add(lines[i]);
          i++;
        }
        if (i < lines.length) i++;
        out.add('<pre><code>${_escapeHtml(code.join('\n'))}</code></pre>');
        continue;
      }
      final heading = RegExp(r'^(#{1,3})\s+(.*)$').firstMatch(line);
      if (heading != null) {
        final level = heading.group(1)!.length;
        out.add('<h$level>${_inline(heading.group(2)!)}</h$level>');
        i++;
        continue;
      }
      if (line.trim() == '---') {
        out.add('<hr>');
        i++;
        continue;
      }
      if (line.startsWith('>')) {
        final buf = <String>[];
        while (i < lines.length && lines[i].startsWith('>')) {
          buf.add(lines[i].replaceFirst(RegExp(r'^>\s?'), ''));
          i++;
        }
        out.add(
            '<blockquote><p>${buf.map(_inline).join('<br>')}</p></blockquote>');
        continue;
      }
      if (RegExp(r'^[-*]\s+').hasMatch(line)) {
        final items = <String>[];
        while (i < lines.length && RegExp(r'^[-*]\s+').hasMatch(lines[i])) {
          items.add(lines[i].replaceFirst(RegExp(r'^[-*]\s+'), ''));
          i++;
        }
        out.add(
            '<ul>${items.map((e) => '<li>${_inline(e)}</li>').join()}</ul>');
        continue;
      }
      if (RegExp(r'^\d+\.\s+').hasMatch(line)) {
        final items = <String>[];
        while (i < lines.length && RegExp(r'^\d+\.\s+').hasMatch(lines[i])) {
          items.add(lines[i].replaceFirst(RegExp(r'^\d+\.\s+'), ''));
          i++;
        }
        out.add(
            '<ol>${items.map((e) => '<li>${_inline(e)}</li>').join()}</ol>');
        continue;
      }
      final para = <String>[];
      while (i < lines.length) {
        final l = lines[i];
        if (l.trim().isEmpty ||
            RegExp(r'^(#{1,3})\s+').hasMatch(l) ||
            l.trim() == '---' ||
            l.startsWith('>') ||
            RegExp(r'^[-*]\s+').hasMatch(l) ||
            RegExp(r'^\d+\.\s+').hasMatch(l) ||
            l.trimLeft().startsWith('```')) {
          break;
        }
        para.add(l);
        i++;
      }
      out.add('<p>${para.map(_inline).join('<br>')}</p>');
    }
    return out.join();
  }

  static bool hasMarkers(String input) {
    final s = input;
    return RegExp(
      r'(\*\*.+\*\*|~~.+~~|`[^`]+`|:[^\s:]+:|\[[^\]]+\]\([^)]+\)|^#{1,3}\s+|^>\s?|^[-*]\s+|^\d+\.\s+|^---+$|```|\$\[[^\]]+\])',
      multiLine: true,
    ).hasMatch(s);
  }

  static String _inline(String input) {
    var out = _escapeHtml(input);
    out = out.replaceAllMapped(RegExp(r'\[([^\]]+)\]\(([^)]+)\)'),
        (m) => '<a href="${m[2]}">${m[1]}</a>');
    out = out.replaceAllMapped(
        RegExp(r'`([^`]+)`'), (m) => '<code>${m[1]}</code>');
    out = out.replaceAllMapped(
        RegExp(r'\*\*([^*]+)\*\*'), (m) => '<strong>${m[1]}</strong>');
    out = out.replaceAllMapped(
        RegExp(r'~~([^~]+)~~'), (m) => '<del>${m[1]}</del>');
    out =
        out.replaceAllMapped(RegExp(r'\*([^*]+)\*'), (m) => '<em>${m[1]}</em>');
    out = out.replaceAllMapped(
        RegExp(r'\$\[(x[0-9.]+)\s+([^\]]*)\]'),
        (m) =>
            '<span style="font-size:${m[1]!.substring(1)}em">${m[2]}</span>');
    out = out.replaceAllMapped(RegExp(r'\$\[font\.serif\s+([^\]]*)\]'),
        (m) => '<span style="font-family:serif">${m[1]}</span>');
    out = out.replaceAllMapped(RegExp(r'\$\[font\.monospace\s+([^\]]*)\]'),
        (m) => '<span style="font-family:monospace">${m[1]}</span>');
    out = out.replaceAllMapped(RegExp(r'\$\[font\.sans-serif\s+([^\]]*)\]'),
        (m) => '<span style="font-family:sans-serif">${m[1]}</span>');
    for (final align in const ['center', 'left', 'right']) {
      out = out.replaceAllMapped(
          RegExp('\\\$\\[$align\\s+([^\\]]*)\\]'),
          (m) =>
              '<span style="display:block;text-align:$align">${m[1]}</span>');
    }
    return out;
  }

  static String _escapeHtml(String input) {
    return input
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;');
  }
}
