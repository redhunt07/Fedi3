/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// Local snapshots of bookmarked notes so the "Saved" screen can render them
/// even when a note is no longer present in the timeline cache.
class SavedNotesStore {
  static Directory? _dirOverride;

  static void useDirectoryForTesting(Directory? dir) {
    _dirOverride = dir;
  }

  static Future<File> _file() async {
    final dir = _dirOverride ?? await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}saved_notes.json');
  }

  static Future<Map<String, dynamic>> _readRaw() async {
    try {
      final f = await _file();
      if (!await f.exists()) return <String, dynamic>{};
      final txt = await f.readAsString();
      final json = jsonDecode(txt);
      if (json is! Map) return <String, dynamic>{};
      return json.cast<String, dynamic>();
    } catch (_) {
      return <String, dynamic>{};
    }
  }

  static Future<void> _writeRaw(Map<String, dynamic> data) async {
    final f = await _file();
    await f.parent.create(recursive: true);
    final tmp = File('${f.path}.tmp');
    await tmp.writeAsString(jsonEncode(data));
    await tmp.rename(f.path);
  }

  static Future<void> add(Map<String, dynamic> activity) async {
    final id = (activity['object'] is Map)
        ? ((activity['object'] as Map)['id']?.toString().trim() ?? '')
        : '';
    final noteId = id.isNotEmpty
        ? id
        : (activity['id']?.toString().trim() ?? '');
    if (noteId.isEmpty) return;
    final raw = await _readRaw();
    raw[noteId] = <String, dynamic>{
      'savedAtMs': DateTime.now().millisecondsSinceEpoch,
      'activity': activity,
    };
    await _writeRaw(raw);
  }

  static Future<void> remove(String noteId) async {
    final id = noteId.trim();
    if (id.isEmpty) return;
    final raw = await _readRaw();
    if (raw.remove(id) != null) await _writeRaw(raw);
  }

  static Future<bool> contains(String noteId) async {
    final raw = await _readRaw();
    return raw.containsKey(noteId.trim());
  }

  /// Newest saved notes first. Each entry contains `id`, `savedAtMs` and
  /// `activity` (the AP activity snapshot).
  static Future<List<Map<String, dynamic>>> readAll() async {
    final raw = await _readRaw();
    final items = <Map<String, dynamic>>[];
    for (final entry in raw.entries) {
      final value = entry.value;
      if (value is! Map) continue;
      final activity = value['activity'];
      if (activity is! Map) continue;
      final savedAt = (value['savedAtMs'] is num)
          ? (value['savedAtMs'] as num).toInt()
          : 0;
      items.add(<String, dynamic>{
        'id': entry.key,
        'savedAtMs': savedAt,
        'activity': activity.cast<String, dynamic>(),
      });
    }
    items.sort((a, b) =>
        (b['savedAtMs'] as int).compareTo(a['savedAtMs'] as int));
    return items;
  }
}
