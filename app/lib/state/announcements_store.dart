/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

class AnnouncementsStore {
  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}announcements.json');
  }

  static Future<Map<String, dynamic>> _read() async {
    try {
      final f = await _file();
      if (!await f.exists()) return <String, dynamic>{};
      final json = jsonDecode(await f.readAsString());
      return json is Map ? json.cast<String, dynamic>() : <String, dynamic>{};
    } catch (_) {
      return <String, dynamic>{};
    }
  }

  static Future<void> _write(Map<String, dynamic> data) async {
    final f = await _file();
    await f.parent.create(recursive: true);
    await f.writeAsString(jsonEncode(data));
  }

  static Future<String?> lastSeenId() async {
    final data = await _read();
    final id = data['last_seen_id'];
    return id is String && id.isNotEmpty ? id : null;
  }

  static Future<void> markSeen(String? id) async {
    if (id == null || id.isEmpty) return;
    final data = await _read();
    data['last_seen_id'] = id;
    await _write(data);
  }

  static Future<String?> cached() async {
    final data = await _read();
    final raw = data['cached_md'];
    return raw is String && raw.isNotEmpty ? raw : null;
  }

  static Future<void> cache(String raw) async {
    final data = await _read();
    data['cached_md'] = raw;
    data['cached_at_ms'] = DateTime.now().millisecondsSinceEpoch;
    await _write(data);
  }
}
