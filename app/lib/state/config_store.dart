/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

/// M-25/AS-01..04: how the configuration file was found on disk.
enum ConfigLoadStatus {
  /// No config file and no backup: genuine first run.
  absent,

  /// Config file parsed successfully.
  ok,

  /// Config file was corrupt/missing but a valid `.bak` was recovered.
  recoveredFromBackup,

  /// Config file exists but is unreadable/unparsable and no backup helped.
  corrupt,
}

class ConfigLoadResult {
  const ConfigLoadResult({required this.status, this.data, this.error});

  final ConfigLoadStatus status;
  final Map<String, dynamic>? data;
  final String? error;

  bool get isOk =>
      status == ConfigLoadStatus.ok ||
      status == ConfigLoadStatus.recoveredFromBackup;
  bool get isCorrupt => status == ConfigLoadStatus.corrupt;
  bool get isAbsent => status == ConfigLoadStatus.absent;
}

class ConfigStore {
  ConfigStore._();

  static Directory? _dirOverride;

  /// Tests point the store at a temp directory.
  @visibleForTesting
  static void useDirectoryForTesting(Directory? dir) {
    _dirOverride = dir;
  }

  static File configFile() {
    final dir = _configDir();
    if (!dir.existsSync()) {
      dir.createSync(recursive: true);
      _restrictDirectory(dir);
    }
    return File(_join(dir.path, 'config.json'));
  }

  static File backupFile() => File('${configFile().path}.bak');
  static File _tempFile() => File('${configFile().path}.tmp');

  /// Reads the config and distinguishes "absent" from "corrupt" (AS-03).
  static ConfigLoadResult load() {
    final f = configFile();
    final bak = backupFile();

    if (f.existsSync()) {
      final parsed = _readJsonFile(f);
      if (parsed != null) {
        return ConfigLoadResult(status: ConfigLoadStatus.ok, data: parsed);
      }
    }

    if (bak.existsSync()) {
      final parsed = _readJsonFile(bak);
      if (parsed != null) {
        // Restore the main file from the last known good backup.
        try {
          _writeAtomically(parsed, keepBackup: false);
        } catch (_) {
          // Recovery still succeeds for this process even if the restore fails.
        }
        return ConfigLoadResult(
            status: ConfigLoadStatus.recoveredFromBackup, data: parsed);
      }
    }

    if (!f.existsSync()) {
      return const ConfigLoadResult(status: ConfigLoadStatus.absent);
    }
    return const ConfigLoadResult(
      status: ConfigLoadStatus.corrupt,
      error: 'config.json is not valid JSON and no valid .bak exists',
    );
  }

  /// Backwards-compatible accessor: returns null for absent/corrupt.
  static Map<String, dynamic>? readConfig() => load().data;

  /// Writes the config atomically: temp file + flush + rename, keeping the
  /// previous content in `config.json.bak`, and restricting permissions to 600
  /// on POSIX (AS-01/AS-04).
  static void writeConfig(Map<String, dynamic> cfg) {
    _writeAtomically(cfg, keepBackup: true);
  }

  static void _writeAtomically(Map<String, dynamic> cfg,
      {required bool keepBackup}) {
    final target = configFile();
    final tmp = _tempFile();
    final encoded = const JsonEncoder.withIndent('  ').convert(cfg);

    if (keepBackup && target.existsSync()) {
      final previous = _readJsonFile(target);
      if (previous != null) {
        final bakTmp = File('${backupFile().path}.tmp');
        bakTmp.writeAsStringSync(
            const JsonEncoder.withIndent('  ').convert(previous),
            flush: true);
        _restrictFile(bakTmp);
        bakTmp.renameSync(backupFile().path);
        _restrictFile(backupFile());
      }
    }

    tmp.writeAsStringSync(encoded, flush: true);
    _restrictFile(tmp);
    tmp.renameSync(target.path);
    _restrictFile(target);
  }

  static void clear() {
    final f = configFile();
    if (f.existsSync()) {
      f.deleteSync();
    }
    final tmp = _tempFile();
    if (tmp.existsSync()) {
      tmp.deleteSync();
    }
    final bak = backupFile();
    if (bak.existsSync()) {
      bak.deleteSync();
    }
  }

  static Map<String, dynamic>? _readJsonFile(File f) {
    try {
      final text = f.readAsStringSync();
      if (text.trim().isEmpty) return null;
      final json = jsonDecode(text);
      if (json is! Map) return null;
      return json.cast<String, dynamic>();
    } catch (_) {
      return null;
    }
  }

  static void _restrictFile(File f) {
    if (Platform.isWindows) return;
    try {
      Process.runSync('chmod', ['600', f.path]);
    } catch (_) {
      // Permissions are best-effort; the atomic write already protects data.
    }
  }

  static void _restrictDirectory(Directory d) {
    if (Platform.isWindows) return;
    try {
      Process.runSync('chmod', ['700', d.path]);
    } catch (_) {}
  }

  static Directory _configDir() {
    if (_dirOverride != null) return _dirOverride!;
    if (Platform.isWindows) {
      final base = Platform.environment['APPDATA'] ??
          Platform.environment['USERPROFILE'] ??
          '.';
      return Directory(_join(base, 'Fedi3'));
    }
    if (Platform.isMacOS) {
      final home = Platform.environment['HOME'] ?? '.';
      return Directory(_join(home, 'Library', 'Application Support', 'Fedi3'));
    }
    final home = Platform.environment['HOME'] ?? '.';
    return Directory(_join(home, '.config', 'fedi3'));
  }

  static String _join(String a, [String? b, String? c, String? d]) {
    final parts =
        [a, b, c, d].whereType<String>().where((p) => p.isNotEmpty).toList();
    final sep = Platform.pathSeparator;
    return parts.join(sep);
  }
}
