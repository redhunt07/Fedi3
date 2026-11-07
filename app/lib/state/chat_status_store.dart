/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// Small local cache of the best known chat message status.
///
/// The core remains the source of truth, but keeping the last known status
/// prevents an outgoing message from falling back to "pending" when a status
/// request is temporarily unavailable or the app is restarted.
class ChatStatusStore {
  static Map<String, String> _memory = <String, String>{};

  static int rank(String status) {
    return switch (status.toLowerCase()) {
      'queued' => 0,
      'sent' => 1,
      'delivered' => 2,
      'read' || 'seen' => 3,
      _ => -1,
    };
  }

  static Future<File?> _file() async {
    try {
      final dir = await getApplicationSupportDirectory();
      return File('${dir.path}${Platform.pathSeparator}chat_status.json');
    } catch (_) {
      return null;
    }
  }

  static Future<Map<String, String>> readAll() async {
    final file = await _file();
    if (file == null) return Map<String, String>.from(_memory);
    try {
      if (!await file.exists()) return Map<String, String>.from(_memory);
      final decoded = jsonDecode(await file.readAsString());
      if (decoded is! Map) return Map<String, String>.from(_memory);
      final result = <String, String>{};
      for (final entry in decoded.entries) {
        final value = entry.value?.toString().trim().toLowerCase() ?? '';
        if (value.isNotEmpty) result[entry.key.toString()] = value;
      }
      _memory = result;
      return Map<String, String>.from(result);
    } catch (_) {
      return Map<String, String>.from(_memory);
    }
  }

  static const int _maxEntries = 2000;

  /// Keeps the cache bounded: the oldest entries are dropped first.
  static void _prune(Map<String, String> entries) {
    if (entries.length <= _maxEntries) return;
    final excess = entries.length - _maxEntries;
    for (final key in entries.keys.take(excess).toList(growable: false)) {
      entries.remove(key);
    }
  }

  static Future<void> merge(Map<String, String> updates) async {
    final current = await readAll();
    for (final entry in updates.entries) {
      final status = entry.value.trim().toLowerCase();
      if (status.isEmpty) continue;
      final previous = current[entry.key];
      if (previous == null || rank(status) >= rank(previous)) {
        current[entry.key] = status;
      }
    }
    _prune(current);
    _memory = Map<String, String>.from(current);

    final file = await _file();
    if (file == null) return;
    try {
      await file.parent.create(recursive: true);
      final tmp = File('${file.path}.tmp');
      await tmp.writeAsString(jsonEncode(current));
      await tmp.rename(file.path);
    } catch (_) {
      // Best-effort cache: in-memory values remain valid for this session.
    }
  }
}
