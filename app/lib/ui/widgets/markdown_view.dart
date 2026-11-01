/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

/// Lightweight Markdown renderer for in-app documents (announcements).
/// Supports headings, quotes, lists, code fences, dividers and inline marks.
class MarkdownView extends StatelessWidget {
  const MarkdownView({super.key, required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    final blocks = _parse(context, text);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: blocks,
    );
  }

  List<Widget> _parse(BuildContext context, String raw) {
    final out = <Widget>[];
    final lines = raw.replaceAll('\r\n', '\n').split('\n');
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
        out.add(Container(
          margin: const EdgeInsets.symmetric(vertical: 6),
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: Theme.of(context).colorScheme.surfaceContainerHighest,
            borderRadius: BorderRadius.circular(10),
          ),
          child: Text(
            code.join('\n'),
            style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
          ),
        ));
        continue;
      }
      final heading = RegExp(r'^(#{1,4})\s+(.*)$').firstMatch(line);
      if (heading != null) {
        final level = heading.group(1)!.length;
        out.add(Padding(
          padding: const EdgeInsets.only(bottom: 6, top: 4),
          child: Text(
            heading.group(2)!,
            style: Theme.of(context).textTheme.titleMedium?.copyWith(
                  fontSize: level == 1 ? 22 : (level == 2 ? 18 : 16),
                  fontWeight: FontWeight.w800,
                ),
          ),
        ));
        i++;
        continue;
      }
      if (line.trim() == '---') {
        out.add(const Divider());
        i++;
        continue;
      }
      if (line.startsWith('>')) {
        final buf = <String>[];
        while (i < lines.length && lines[i].startsWith('>')) {
          buf.add(lines[i].replaceFirst(RegExp(r'^>\s?'), ''));
          i++;
        }
        out.add(Container(
          margin: const EdgeInsets.symmetric(vertical: 6),
          padding: const EdgeInsets.only(left: 12),
          decoration: BoxDecoration(
            border: Border(
              left: BorderSide(
                  color: Theme.of(context).colorScheme.primary, width: 3),
            ),
          ),
          child: Text(buf.join('\n')),
        ));
        continue;
      }
      if (RegExp(r'^[-*]\s+').hasMatch(line)) {
        final items = <String>[];
        while (i < lines.length && RegExp(r'^[-*]\s+').hasMatch(lines[i])) {
          items.add(lines[i].replaceFirst(RegExp(r'^[-*]\s+'), ''));
          i++;
        }
        out.add(Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final item in items)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text('•  '),
                    Expanded(child: _inline(context, item)),
                  ],
                ),
              ),
          ],
        ));
        continue;
      }
      if (RegExp(r'^\d+\.\s+').hasMatch(line)) {
        final items = <String>[];
        while (i < lines.length && RegExp(r'^\d+\.\s+').hasMatch(lines[i])) {
          items.add(lines[i].replaceFirst(RegExp(r'^\d+\.\s+'), ''));
          i++;
        }
        var n = 1;
        out.add(Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final item in items)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('$n.  '),
                    Expanded(child: _inline(context, item)),
                  ],
                ),
              ),
          ],
        ));
        continue;
      }
      final para = <String>[];
      while (i < lines.length) {
        final l = lines[i];
        if (l.trim().isEmpty ||
            RegExp(r'^#{1,4}\s+').hasMatch(l) ||
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
      out.add(Padding(
        padding: const EdgeInsets.only(bottom: 8),
        child: _inline(context, para.join('\n')),
      ));
    }
    return out;
  }

  Widget _inline(BuildContext context, String text) {
    final spans = <InlineSpan>[];
    final pattern = RegExp(
        r'(\*\*[^*]+\*\*|\*[^*]+\*|~~[^~]+~~|`[^`]+`|\[[^\]]+\]\([^)]+\))');
    var last = 0;
    for (final m in pattern.allMatches(text)) {
      if (m.start > last) {
        spans.add(TextSpan(text: text.substring(last, m.start)));
      }
      final token = m.group(0)!;
      if (token.startsWith('**')) {
        spans.add(TextSpan(
            text: token.substring(2, token.length - 2),
            style: const TextStyle(fontWeight: FontWeight.w800)));
      } else if (token.startsWith('~~')) {
        spans.add(TextSpan(
            text: token.substring(2, token.length - 2),
            style: const TextStyle(decoration: TextDecoration.lineThrough)));
      } else if (token.startsWith('`')) {
        spans.add(TextSpan(
            text: token.substring(1, token.length - 1),
            style: const TextStyle(fontFamily: 'monospace')));
      } else if (token.startsWith('*')) {
        spans.add(TextSpan(
            text: token.substring(1, token.length - 1),
            style: const TextStyle(fontStyle: FontStyle.italic)));
      } else {
        final link = RegExp(r'\[([^\]]+)\]\(([^)]+)\)').firstMatch(token);
        if (link != null) {
          spans.add(TextSpan(
            text: link.group(1),
            style: TextStyle(
              color: Theme.of(context).colorScheme.primary,
              decoration: TextDecoration.underline,
            ),
          ));
        } else {
          spans.add(TextSpan(text: token));
        }
      }
      last = m.end;
    }
    if (last < text.length) {
      spans.add(TextSpan(text: text.substring(last)));
    }
    return Text.rich(TextSpan(children: spans));
  }
}
