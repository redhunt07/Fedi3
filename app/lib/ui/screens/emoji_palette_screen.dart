/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../state/emoji_palette_store.dart';

class EmojiPaletteScreen extends StatefulWidget {
  const EmojiPaletteScreen({super.key});

  @override
  State<EmojiPaletteScreen> createState() => _EmojiPaletteScreenState();
}

class _EmojiPaletteScreenState extends State<EmojiPaletteScreen> {
  final TextEditingController _input = TextEditingController();
  List<String> _palette = const [];

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _input.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final list = await EmojiPaletteStore.read();
    if (!mounted) return;
    setState(() => _palette = list);
  }

  Future<void> _save() async {
    await EmojiPaletteStore.write(_palette);
  }

  void _add() {
    final e = _input.text.trim();
    if (e.isEmpty) return;
    setState(() {
      _palette = [..._palette, e];
      _input.clear();
    });
    _save();
  }

  void _removeAt(int index) {
    setState(() {
      _palette = [..._palette]..removeAt(index);
    });
    _save();
  }

  void _swap(int a, int b) {
    if (a < 0 || b < 0 || a >= _palette.length || b >= _palette.length) return;
    final list = [..._palette];
    final tmp = list[a];
    list[a] = list[b];
    list[b] = tmp;
    setState(() => _palette = list);
    _save();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.uiEmojiPaletteTitle)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          TextField(
            controller: _input,
            decoration: InputDecoration(
              labelText: context.l10n.emojiPaletteAddLabel,
              hintText: context.l10n.emojiPaletteAddHint,
            ),
            onSubmitted: (_) => _add(),
          ),
          const SizedBox(height: 12),
          FilledButton.icon(
            onPressed: _add,
            icon: const Icon(Icons.add),
            label: Text(context.l10n.emojiPaletteAddButton),
          ),
          const SizedBox(height: 16),
          if (_palette.isEmpty)
            Text(context.l10n.emojiPaletteEmpty)
          else
            for (var i = 0; i < _palette.length; i++)
              Card(
                child: ListTile(
                  title: Text(_palette[i], style: const TextStyle(fontSize: 22)),
                  trailing: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      IconButton(
                        tooltip: 'Move up',
                        onPressed: i == 0 ? null : () => _swap(i, i - 1),
                        icon: const Icon(Icons.keyboard_arrow_up),
                      ),
                      IconButton(
                        tooltip: 'Move down',
                        onPressed: i == _palette.length - 1 ? null : () => _swap(i, i + 1),
                        icon: const Icon(Icons.keyboard_arrow_down),
                      ),
                      IconButton(
                        tooltip: 'Remove',
                        onPressed: () => _removeAt(i),
                        icon: const Icon(Icons.delete_outline),
                      ),
                    ],
                  ),
                ),
              ),
        ],
      ),
    );
  }
}
