/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

class MfmCodec {
  static String toHtml(String input) {
    final raw = input.replaceAll('\r\n', '\n').replaceAll('\r', '\n').trim();
    if (raw.isEmpty) return '';
    final escaped = _escapeHtml(raw);
    final formatted = _applyInlineFormatting(escaped);
    return '<p>${formatted.replaceAll('\n', '<br>')}</p>';
  }

  static bool hasMarkers(String input) {
    final s = input;
    return RegExp(r'(\*\*.+\*\*|~~.+~~|`[^`]+`|:[^\s:]+:)').hasMatch(s);
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
    return out;
  }
}
