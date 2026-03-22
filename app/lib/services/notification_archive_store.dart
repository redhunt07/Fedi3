/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

class NotificationArchiveStore {
  NotificationArchiveStore._();

  static final NotificationArchiveStore instance = NotificationArchiveStore._();

  static const int _maxArchiveItems = 2000;
  static const int _maxSidebarItems = 10;

  Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File(
        '${dir.path}${Platform.pathSeparator}notifications_archive.json');
  }

  Future<List<Map<String, dynamic>>> readArchive() async {
    try {
      final file = await _file();
      if (!await file.exists()) return const [];
      final txt = await file.readAsString();
      final json = jsonDecode(txt);
      if (json is! List) return const [];
      return json
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .toList(growable: false);
    } catch (_) {
      return const [];
    }
  }

  Future<void> writeArchive(List<Map<String, dynamic>> items) async {
    final file = await _file();
    await file.parent.create(recursive: true);
    await file.writeAsString(jsonEncode(items.take(_maxArchiveItems).toList()));
  }

  Future<List<Map<String, dynamic>>> mergeAndPersist(
    List<Map<String, dynamic>> incoming,
  ) async {
    final current = await readArchive();
    final byKey = <String, Map<String, dynamic>>{};
    for (final item in [...incoming, ...current]) {
      final key = _itemKey(item);
      if (key.isEmpty) continue;
      final prev = byKey[key];
      if (prev == null || _itemTs(item) >= _itemTs(prev)) {
        byKey[key] = item;
      }
    }
    final merged = byKey.values.toList()
      ..sort((a, b) => _itemTs(b).compareTo(_itemTs(a)));
    await writeArchive(merged);
    return merged;
  }

  List<Map<String, dynamic>> sidebarQueue(List<Map<String, dynamic>> archive) {
    return archive.take(_maxSidebarItems).toList(growable: false);
  }

  String _itemKey(Map<String, dynamic> item) {
    final activity = item['activity'];
    if (activity is Map) {
      final id = (activity['id'] as String?)?.trim() ?? '';
      if (id.isNotEmpty) return 'activity:$id';
    }
    final ts = _itemTs(item);
    final actor =
        (activity is Map ? activity['actor'] : null)?.toString() ?? '';
    final ty = (activity is Map ? activity['type'] : null)?.toString() ?? '';
    if (ts > 0 && actor.isNotEmpty && ty.isNotEmpty) {
      return '$ts|$actor|$ty';
    }
    return '';
  }

  int _itemTs(Map<String, dynamic> item) {
    final raw = item['ts'];
    if (raw is num) return raw.toInt();
    if (raw is String) return int.tryParse(raw.trim()) ?? 0;
    return 0;
  }
}
