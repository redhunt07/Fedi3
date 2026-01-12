/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../state/emoji_palette_store.dart';
import '../../state/emoji_recent_store.dart';
import '../../state/emoji_store.dart';
import '../../model/ui_prefs.dart';
import '../../l10n/l10n_ext.dart';

class EmojiPickerResult {
  const EmojiPickerResult(this.emoji);
  final String emoji;
}

class EmojiPicker extends StatefulWidget {
  const EmojiPicker({
    super.key,
    required this.scale,
    required this.columns,
    required this.style,
  });

  final double scale;
  final int columns;
  final EmojiPickerStyle style;

  static Future<String?> show(BuildContext context, {required UiPrefs prefs}) async {
    final res = await showModalBottomSheet<EmojiPickerResult>(
      context: context,
      isScrollControlled: true,
      builder: (context) => SafeArea(
        child: EmojiPicker(
          scale: prefs.emojiPickerScale,
          columns: prefs.emojiPickerColumns,
          style: prefs.emojiPickerStyle,
        ),
      ),
    );
    return res?.emoji;
  }

  @override
  State<EmojiPicker> createState() => _EmojiPickerState();
}

class _EmojiPickerState extends State<EmojiPicker> {
  final TextEditingController _search = TextEditingController();
  List<String> _palette = const [];
  List<String> _recent = const [];
  Map<String, String> _globalCustom = const {};
  String _query = '';

  static const List<String> _common = [
    '👍',
    '❤️',
    '😂',
    '😮',
    '😢',
    '😡',
    '🎉',
    '🔥',
    '🙏',
    '👀',
    '💯',
    '✨',
    '🤔',
    '👏',
  ];

  @override
  void initState() {
    super.initState();
    _load();
    _search.addListener(() {
      final q = _search.text.trim();
      if (q == _query) return;
      setState(() => _query = q);
    });
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final palette = await EmojiPaletteStore.read();
    final recent = await EmojiRecentStore.read();
    final global = await EmojiStore.read();
    if (!mounted) return;
    setState(() {
      _palette = palette;
      _recent = recent;
      _globalCustom = global;
    });
  }

  void _pick(String emoji) {
    Navigator.of(context).pop(EmojiPickerResult(emoji));
  }

  List<String> _filter(List<String> input) {
    final query = _query.trim().toLowerCase();
    if (query.isEmpty) return input;
    return input.where((e) => e.toLowerCase().contains(query)).toList(growable: false);
  }

  List<MapEntry<String, String>> _filterCustom(Map<String, String> input) {
    final entries = input.entries
        .where((e) => e.key.trim().isNotEmpty && e.value.trim().isNotEmpty)
        .toList(growable: false)
      ..sort((a, b) => a.key.compareTo(b.key));
    final query = _query.trim().toLowerCase();
    if (query.isEmpty) return entries;
    return entries
        .where((e) {
          final name = e.key.toLowerCase();
          return name.contains(query) || name.replaceAll(':', '').contains(query);
        })
        .toList(growable: false);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    Widget section(String title, List<String> emojis) {
      if (emojis.isEmpty) return const SizedBox.shrink();
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 10),
          LayoutBuilder(
            builder: (context, constraints) {
              const spacing = 8.0;
              final columns = widget.columns.clamp(4, 12);
              final size = ((constraints.maxWidth - spacing * (columns - 1)) / columns).clamp(32.0, 64.0);
              final font = (20.0 * widget.scale).clamp(16.0, 30.0);
              return Wrap(
                spacing: spacing,
                runSpacing: spacing,
                children: [
                  for (final e in emojis)
                    InkWell(
                      onTap: () => _pick(e),
                      borderRadius: BorderRadius.circular(12),
                      child: Container(
                        width: size,
                        height: size,
                        alignment: Alignment.center,
                        decoration: BoxDecoration(
                          color: theme.colorScheme.surfaceContainerHighest,
                          borderRadius: BorderRadius.circular(12),
                        ),
                        child: Text(e, style: TextStyle(fontSize: font)),
                      ),
                    ),
                ],
              );
            },
          ),
        ],
      );
    }

    Widget customSection(String title, List<MapEntry<String, String>> emojis) {
      if (emojis.isEmpty) return const SizedBox.shrink();
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 10),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              for (final e in emojis)
                InkWell(
                  onTap: () => _pick(e.key),
                  borderRadius: BorderRadius.circular(12),
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                    decoration: BoxDecoration(
                      color: theme.colorScheme.surfaceContainerHighest,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (widget.style == EmojiPickerStyle.text)
                          Text(e.key, style: const TextStyle(fontSize: 16))
                        else
                          Image.network(
                            e.value,
                            width: 22,
                            height: 22,
                            fit: BoxFit.contain,
                            errorBuilder: (_, __, ___) => Text(e.key, style: const TextStyle(fontSize: 16)),
                          ),
                        const SizedBox(width: 8),
                        Text(e.key, style: const TextStyle(fontWeight: FontWeight.w700)),
                      ],
                    ),
                  ),
                ),
            ],
          ),
        ],
      );
    }

    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(child: Text(context.l10n.emojiPickerTitle, style: const TextStyle(fontWeight: FontWeight.w900, fontSize: 16))),
              IconButton(
                tooltip: context.l10n.emojiPickerClose,
                onPressed: () => Navigator.of(context).pop(),
                icon: const Icon(Icons.close),
              ),
            ],
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _search,
            decoration: InputDecoration(
              labelText: context.l10n.emojiPickerSearchLabel,
              hintText: context.l10n.emojiPickerSearchHint,
            ),
            onSubmitted: (v) {
              final e = v.trim();
              if (e.isEmpty) return;
              _pick(e);
            },
          ),
          const SizedBox(height: 14),
          Flexible(
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  section(context.l10n.emojiPickerPalette, _filter(_palette)),
                  if (_palette.isNotEmpty) const SizedBox(height: 18),
                  section(context.l10n.emojiPickerRecent, _filter(_recent)),
                  if (_recent.isNotEmpty) const SizedBox(height: 18),
                  section(context.l10n.emojiPickerCommon, _filter(_common)),
                  if (_globalCustom.isNotEmpty) ...[
                    const SizedBox(height: 18),
                    customSection(context.l10n.emojiPickerCustom, _filterCustom(_globalCustom)),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
