/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

class EmojiPaletteStore {
  static const List<String> defaults = [
    '😀',
    '😄',
    '😍',
    '🥳',
    '✨',
    '🔥',
    '👍',
    '❤️',
    '🙏',
    '👀',
  ];

  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}emoji_palette.json');
  }

  static Future<List<String>> read() async {
    try {
      final f = await _file();
      if (!await f.exists()) return defaults;
      final txt = await f.readAsString();
      final json = jsonDecode(txt);
      if (json is! List) return defaults;
      final list = json.whereType<String>().map((s) => s.trim()).where((s) => s.isNotEmpty).toList();
      return list.isEmpty ? defaults : list;
    } catch (_) {
      return defaults;
    }
  }

  static Future<void> write(List<String> emojis) async {
    final list = emojis.map((e) => e.trim()).where((e) => e.isNotEmpty).toList();
    final f = await _file();
    await f.parent.create(recursive: true);
    await f.writeAsString(jsonEncode(list));
  }
}
