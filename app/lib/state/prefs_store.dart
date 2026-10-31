/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import '../model/ui_prefs.dart';

/// M-25/AS-05: outcome of reading the local preferences file.
class PrefsLoadResult {
  const PrefsLoadResult(
      {required this.prefs, required this.corrupt, this.error});

  final UiPrefs prefs;

  /// True when the file existed but could not be parsed (defaults are used).
  final bool corrupt;
  final String? error;
}

class PrefsStore {
  static Directory? _dirOverride;

  /// Tests point the store at a temp directory (path_provider is unavailable).
  @visibleForTesting
  static void useDirectoryForTesting(Directory? dir) {
    _dirOverride = dir;
  }

  static Future<File> _file() async {
    final dir = _dirOverride ?? await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}ui_prefs.json');
  }

  static Future<File> _backupFile() async {
    final f = await _file();
    return File('${f.path}.bak');
  }

  /// Reads preferences, distinguishing a missing file from a corrupt one.
  static Future<PrefsLoadResult> load() async {
    try {
      final f = await _file();
      if (await f.exists()) {
        final parsed = await _readPrefsFile(f);
        if (parsed != null) {
          return PrefsLoadResult(prefs: parsed, corrupt: false);
        }
      }
      final bak = await _backupFile();
      if (await bak.exists()) {
        final parsed = await _readPrefsFile(bak);
        if (parsed != null) {
          try {
            await _writeAtomically(parsed, keepBackup: false);
          } catch (_) {}
          return PrefsLoadResult(prefs: parsed, corrupt: false);
        }
      }
      final existed = await f.exists();
      return PrefsLoadResult(
        prefs: UiPrefs.defaults(),
        corrupt: existed,
        error: existed ? 'ui_prefs.json is not valid JSON' : null,
      );
    } catch (e) {
      return PrefsLoadResult(
          prefs: UiPrefs.defaults(), corrupt: false, error: e.toString());
    }
  }

  /// Backwards-compatible accessor.
  static Future<UiPrefs> read() async => (await load()).prefs;

  /// Writes preferences atomically (temp + flush + rename) with a `.bak` and
  /// 600 permissions on POSIX.
  static Future<void> write(UiPrefs prefs) async {
    await _writeAtomically(prefs, keepBackup: true);
  }

  static Future<void> _writeAtomically(UiPrefs prefs,
      {required bool keepBackup}) async {
    final f = await _file();
    await f.parent.create(recursive: true);
    final tmp = File('${f.path}.tmp');
    final encoded = jsonEncode(prefs.toJson());

    if (keepBackup && await f.exists()) {
      final previous = await _readPrefsFile(f);
      if (previous != null) {
        final bak = await _backupFile();
        final bakTmp = File('${bak.path}.tmp');
        await bakTmp.writeAsString(jsonEncode(previous.toJson()), flush: true);
        await _restrict(bakTmp);
        await bakTmp.rename(bak.path);
        await _restrict(bak);
      }
    }

    await tmp.writeAsString(encoded, flush: true);
    await _restrict(tmp);
    await tmp.rename(f.path);
    await _restrict(f);
  }

  static Future<UiPrefs?> _readPrefsFile(File f) async {
    try {
      final txt = await f.readAsString();
      if (txt.trim().isEmpty) return null;
      final json = jsonDecode(txt);
      if (json is! Map) return null;
      return UiPrefs.fromJson(json.cast<String, dynamic>());
    } catch (_) {
      return null;
    }
  }

  static Future<void> _restrict(File f) async {
    if (Platform.isWindows) return;
    try {
      await Process.run('chmod', ['600', f.path]);
    } catch (_) {}
  }
}
