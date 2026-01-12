/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

class ConfigStore {
  ConfigStore._();

  static File configFile() {
    final dir = _configDir();
    if (!dir.existsSync()) {
      dir.createSync(recursive: true);
    }
    return File(_join(dir.path, 'config.json'));
  }

  static Map<String, dynamic>? readConfig() {
    final f = configFile();
    if (!f.existsSync()) return null;
    try {
      final text = f.readAsStringSync();
      if (text.trim().isEmpty) return null;
      return jsonDecode(text) as Map<String, dynamic>;
    } catch (_) {
      return null;
    }
  }

  static void writeConfig(Map<String, dynamic> cfg) {
    final f = configFile();
    f.writeAsStringSync(const JsonEncoder.withIndent('  ').convert(cfg));
  }

  static void clear() {
    final f = configFile();
    if (f.existsSync()) {
      f.deleteSync();
    }
  }

  static Directory _configDir() {
    if (Platform.isWindows) {
      final base = Platform.environment['APPDATA'] ?? Platform.environment['USERPROFILE'] ?? '.';
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
    final parts = [a, b, c, d].whereType<String>().where((p) => p.isNotEmpty).toList();
    final sep = Platform.pathSeparator;
    return parts.join(sep);
  }
}

