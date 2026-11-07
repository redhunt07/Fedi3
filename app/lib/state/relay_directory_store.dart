/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

/// Persists the last good copy of the shared relay directory so the client can
/// show relays on startup even when GitHub is unreachable.
class RelayDirectoryStore {
  static Directory? _dirOverride;

  @visibleForTesting
  static void useDirectoryForTesting(Directory? dir) {
    _dirOverride = dir;
  }

  static Future<File> _file() async {
    final dir = _dirOverride ?? await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}relay_directory.json');
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
    final tmp = File('${f.path}.tmp');
    await tmp.writeAsString(jsonEncode(data), flush: true);
    await tmp.rename(f.path);
  }

  static Future<String?> cachedRaw() async {
    final data = await _read();
    final raw = data['raw'];
    return raw is String && raw.trim().isNotEmpty ? raw : null;
  }

  static Future<String?> cachedSourceUrl() async {
    final data = await _read();
    final value = data['source_url'];
    return value is String && value.trim().isNotEmpty ? value : null;
  }

  static Future<void> cache(String raw, {required String sourceUrl}) async {
    final data = await _read();
    data['raw'] = raw;
    data['source_url'] = sourceUrl;
    data['cached_at_ms'] = DateTime.now().millisecondsSinceEpoch;
    await _write(data);
  }

  static Future<void> clear() async {
    try {
      final f = await _file();
      if (await f.exists()) await f.delete();
    } catch (_) {}
  }
}
