/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

import '../model/ui_prefs.dart';

class PrefsStore {
  static Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}ui_prefs.json');
  }

  static Future<UiPrefs> read() async {
    try {
      final f = await _file();
      if (!await f.exists()) return UiPrefs.defaults();
      final txt = await f.readAsString();
      final json = jsonDecode(txt);
      if (json is! Map) return UiPrefs.defaults();
      return UiPrefs.fromJson(json.cast<String, dynamic>());
    } catch (_) {
      return UiPrefs.defaults();
    }
  }

  static Future<void> write(UiPrefs prefs) async {
    final f = await _file();
    await f.parent.create(recursive: true);
    await f.writeAsString(jsonEncode(prefs.toJson()));
  }
}

