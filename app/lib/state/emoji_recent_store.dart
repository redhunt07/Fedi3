/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

class EmojiRecentStore {
  static const int _max = 32;

  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}emoji_recent.json');
  }

  static Future<List<String>> read() async {
    try {
      final f = await _file();
      if (!await f.exists()) return const [];
      final txt = await f.readAsString();
      final json = jsonDecode(txt);
      if (json is! List) return const [];
      return json.whereType<String>().map((s) => s.trim()).where((s) => s.isNotEmpty).take(_max).toList();
    } catch (_) {
      return const [];
    }
  }

  static Future<void> add(String emoji) async {
    final e = emoji.trim();
    if (e.isEmpty) return;
    final list = (await read()).toList();
    list.removeWhere((x) => x == e);
    list.insert(0, e);
    while (list.length > _max) {
      list.removeLast();
    }
    final f = await _file();
    await f.parent.create(recursive: true);
    await f.writeAsString(jsonEncode(list));
  }
}
