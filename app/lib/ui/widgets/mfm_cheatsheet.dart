/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';

class MfmCheatsheet {
  static Future<void> show(BuildContext context) {
    return showDialog<void>(
      context: context,
      builder: (context) {
        final lines = context.l10n.composeMfmCheatsheetBody
            .split('\n')
            .map((line) => line.trim())
            .where((line) => line.isNotEmpty)
            .toList(growable: false);
        return AlertDialog(
          title: Text(context.l10n.composeMfmCheatsheetTitle),
          content: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 520),
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final line in lines)
                    _CheatRow(line: line),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: Text(context.l10n.activityCancel),
            ),
          ],
        );
      },
    );
  }
}

class _CheatRow extends StatelessWidget {
  const _CheatRow({required this.line});

  final String line;

  @override
  Widget build(BuildContext context) {
    final parts = line.split('->');
    if (parts.length < 2) {
      return Padding(
        padding: const EdgeInsets.only(bottom: 6),
        child: Text(line, style: const TextStyle(fontFamily: 'monospace')),
      );
    }
    final example = parts.first.trim();
    final desc = parts.sublist(1).join('->').trim();
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 140, maxWidth: 180),
            child: SelectableText(
              example,
              style: const TextStyle(fontFamily: 'monospace'),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Text(desc),
          ),
        ],
      ),
    );
  }
}
