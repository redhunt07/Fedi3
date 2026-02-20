/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

class MfmCodec {
  static String toHtml(String input) {
    final raw = input.replaceAll('\r\n', '\n').replaceAll('\r', '\n').trim();
    if (raw.isEmpty) return '';
    final escaped = _escapeHtml(raw);
    final codeBlocks = <String>[];
    final withoutBlocks = escaped.replaceAllMapped(RegExp(r'```([\s\S]*?)```'), (m) {
      codeBlocks.add(m[1] ?? '');
      return '\u0000CODEBLOCK${codeBlocks.length - 1}\u0000';
    });
    final formatted = _applyInlineFormatting(withoutBlocks);
    return _wrapBlocks(formatted, codeBlocks);
  }

  static bool hasMarkers(String input) {
    final s = input;
    return RegExp(r'(\*\*.+\*\*|~~.+~~|`[^`]+`|```[\s\S]+```|^>.+|:[^\s:]+:|\[[^\]]+\]\([^)]+\))', multiLine: true).hasMatch(s);
  }

  static String _escapeHtml(String input) {
    return input
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;');
  }

  static String _applyInlineFormatting(String input) {
    var out = input;
    out = out.replaceAllMapped(RegExp(r'`([^`]+)`'), (m) => '<code>${m[1]}</code>');
    out = out.replaceAllMapped(RegExp(r'\*\*([^*]+)\*\*'), (m) => '<strong>${m[1]}</strong>');
    out = out.replaceAllMapped(RegExp(r'~~([^~]+)~~'), (m) => '<del>${m[1]}</del>');
    out = out.replaceAllMapped(RegExp(r'\*([^*]+)\*'), (m) => '<em>${m[1]}</em>');
    out = out.replaceAllMapped(RegExp(r'\[([^\]]+)\]\(([^)]+)\)'), (m) => '<a href="${m[2]}">${m[1]}</a>');
    return out;
  }

  static String _wrapBlocks(String input, List<String> codeBlocks) {
    final lines = input.split('\n');
    final buffer = StringBuffer();
    var inQuote = false;
    for (final line in lines) {
      final codeMatch = RegExp(r'\u0000CODEBLOCK(\d+)\u0000').firstMatch(line);
      if (codeMatch != null) {
        if (inQuote) {
          buffer.write('</blockquote>');
          inQuote = false;
        }
        final idx = int.tryParse(codeMatch[1] ?? '');
        if (idx != null && idx >= 0 && idx < codeBlocks.length) {
          final block = codeBlocks[idx];
          buffer.write('<pre><code>$block</code></pre>');
        }
        continue;
      }

      final isQuote = line.startsWith('&gt;');
      if (isQuote && !inQuote) {
        buffer.write('<blockquote>');
        inQuote = true;
      }
      if (!isQuote && inQuote) {
        buffer.write('</blockquote>');
        inQuote = false;
      }

      if (isQuote) {
        var content = line.replaceFirst(RegExp(r'^&gt;\s?'), '');
        if (content.isEmpty) content = '&nbsp;';
        buffer.write(content);
        buffer.write('<br>');
      } else {
        buffer.write(line);
        buffer.write('<br>');
      }
    }
    if (inQuote) buffer.write('</blockquote>');
    var html = buffer.toString();
    if (html.endsWith('<br>')) {
      html = html.substring(0, html.length - 4);
    }
    if (html.contains('<blockquote>') || html.contains('<pre><code>')) {
      return html;
    }
    return '<p>$html</p>';
  }
}
