/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import '../model/core_config.dart';
import '../model/ui_prefs.dart';

class BackupBundle {
  BackupBundle({required this.config, required this.prefs});

  final CoreConfig config;
  final UiPrefs prefs;
}

class BackupCodec {
  static const int version = 1;

  static String encode({required CoreConfig config, required UiPrefs prefs}) {
    final data = {
      'v': version,
      'coreConfig': config.toJson(),
      'uiPrefs': prefs.toJson(),
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
    if (cfgJson == null || prefsJson == null) throw const FormatException('missing sections');

    return BackupBundle(
      config: CoreConfig.fromJson(cfgJson),
      prefs: UiPrefs.fromJson(prefsJson),
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

