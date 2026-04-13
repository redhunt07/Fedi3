/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

import '../model/core_config.dart';
import '../model/relay_sync_models.dart';

class RelaySyncStore {
  static Future<File> _file(CoreConfig config) async {
    final dir = await getApplicationSupportDirectory();
    final safeKey = '${config.username}@${config.domain}'
        .toLowerCase()
        .replaceAll(RegExp(r'[^a-z0-9._@-]+'), '_');
    return File(
      '${dir.path}${Platform.pathSeparator}relay_sync_$safeKey.json',
    );
  }

  static Future<RelaySyncBootstrapSnapshot?> read(CoreConfig config) async {
    try {
      final file = await _file(config);
      if (!await file.exists()) return null;
      final json = jsonDecode(await file.readAsString());
      if (json is! Map) return null;
      return RelaySyncBootstrapSnapshot.fromJson(json.cast<String, dynamic>());
    } catch (_) {
      return null;
    }
  }

  static Future<void> write(
    CoreConfig config,
    RelaySyncBootstrapSnapshot snapshot,
  ) async {
    final file = await _file(config);
    await file.parent.create(recursive: true);
    await file.writeAsString(jsonEncode(snapshot.toJson()));
  }

  static Future<void> clear(CoreConfig config) async {
    final file = await _file(config);
    if (await file.exists()) {
      await file.delete();
    }
  }
}
