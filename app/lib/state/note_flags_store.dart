/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

class NoteFlagsStore {
  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}note_flags.json');
  }

  static Future<Map<String, List<String>>> _readRaw() async {
    try {
      final f = await _file();
      if (!await f.exists()) return {};
      final txt = await f.readAsString();
      final json = jsonDecode(txt);
      if (json is! Map) return {};
      final out = <String, List<String>>{};
      for (final entry in json.entries) {
        if (entry.value is! List) continue;
        out[entry.key.toString()] = (entry.value as List)
            .whereType<String>()
            .map((s) => s.trim())
            .where((s) => s.isNotEmpty)
            .toList();
      }
      return out;
    } catch (_) {
      return {};
    }
  }

  static Future<void> _writeRaw(Map<String, List<String>> data) async {
    final f = await _file();
    await f.parent.create(recursive: true);
    await f.writeAsString(jsonEncode(data));
  }

  static Future<Set<String>> _getSet(String key) async {
    final raw = await _readRaw();
    return raw[key]?.toSet() ?? <String>{};
  }

  static Future<bool> contains(String key, String id) async {
    final s = await _getSet(key);
    return s.contains(id);
  }

  static Future<bool> toggle(String key, String id) async {
    final raw = await _readRaw();
    final set = raw[key]?.toSet() ?? <String>{};
    final next = !set.contains(id);
    if (next) {
      set.add(id);
    } else {
      set.remove(id);
    }
    raw[key] = set.toList();
    await _writeRaw(raw);
    return next;
  }
}
