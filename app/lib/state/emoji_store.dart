/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

import '../model/note_models.dart';

class EmojiStore {
  static const int _max = 400;

  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}emoji_cache.json');
  }

  static Future<Map<String, String>> read() async {
    try {
      final f = await _file();
      if (!await f.exists()) return const {};
      final txt = await f.readAsString();
      final json = jsonDecode(txt);
      if (json is! Map) return const {};
      return json.map((k, v) => MapEntry(k.toString(), v?.toString() ?? ''))
        ..removeWhere((k, v) => k.trim().isEmpty || v.trim().isEmpty);
    } catch (_) {
      return const {};
    }
  }

  static Future<void> addAll(List<NoteEmoji> emojis) async {
    if (emojis.isEmpty) return;
    final next = Map<String, String>.from(await read());
    var changed = false;
    for (final e in emojis) {
      final name = e.name.trim();
      final url = e.iconUrl.trim();
      if (name.isEmpty || url.isEmpty) continue;
      if (next[name] != url) {
        next[name] = url;
        changed = true;
      }
    }
    if (!changed) return;
    while (next.length > _max) {
      next.remove(next.keys.first);
    }
    final f = await _file();
    await f.parent.create(recursive: true);
    await f.writeAsString(jsonEncode(next));
  }
}
