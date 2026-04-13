/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import '../model/core_config.dart';
import '../model/ui_prefs.dart';

class BackupBundle {
  BackupBundle({
    required this.config,
    required this.prefs,
    this.coreBackup,
    this.encryptionKeys,
    this.meta,
  });

  final CoreConfig config;
  final UiPrefs prefs;
  final Map<String, dynamic>? coreBackup;
  final Map<String, String>? encryptionKeys;
  final Map<String, dynamic>? meta;
}

class BackupCodec {
  static const int version = 2;

  static String encode({
    required CoreConfig config,
    required UiPrefs prefs,
    Map<String, dynamic>? coreBackup,
    Map<String, String>? encryptionKeys,
    Map<String, dynamic>? meta,
  }) {
    final data = {
      'v': version,
      'coreConfig': config.toBackupJson(),
      'uiPrefs': prefs.toBackupJson(),
      if (coreBackup != null) 'coreBackup': coreBackup,
      if (encryptionKeys != null) 'encryptionKeys': encryptionKeys,
      if (meta != null) 'meta': meta,
    };
    return const JsonEncoder.withIndent('  ').convert(data);
  }

  static BackupBundle decode(String raw) {
    final txt = raw.trim();
    if (txt.isEmpty) throw const FormatException('empty');
    final json = jsonDecode(txt);
    if (json is! Map) throw const FormatException('invalid json');

    final cfgJson = (json['coreConfig'] as Map?)?.cast<String, dynamic>();
    final prefsJson = (json['uiPrefs'] as Map?)?.cast<String, dynamic>();
    if (cfgJson == null || prefsJson == null) {
      throw const FormatException('missing sections');
    }
    final coreBackup = (json['coreBackup'] as Map?)?.cast<String, dynamic>();
    final rawKeys = (json['encryptionKeys'] as Map?)?.cast<String, dynamic>();
    final encryptionKeys = rawKeys?.map(
      (key, value) => MapEntry(key, value.toString()),
    );
    final meta = (json['meta'] as Map?)?.cast<String, dynamic>();

    return BackupBundle(
      config: CoreConfig.fromJson(cfgJson),
      prefs: UiPrefs.fromJson(prefsJson),
      coreBackup: coreBackup,
      encryptionKeys: encryptionKeys,
      meta: meta,
    );
  }

  static String suggestedFileName(CoreConfig config) {
    String safe(String s) {
      final v = s.trim().replaceAll(RegExp(r'[^a-zA-Z0-9._-]+'), '_');
      return v.isEmpty ? 'unknown' : v;
    }

    final user = safe(config.username);
    final domain = safe(config.domain);
    return 'fedi3-backup-$user@$domain.json';
  }
}
