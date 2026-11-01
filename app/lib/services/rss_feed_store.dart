/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

class RssFeedStore {
  RssFeedStore._();

  static final RssFeedStore instance = RssFeedStore._();

  Future<File> _file() async {
    final dir = await getApplicationSupportDirectory();
    return File('${dir.path}${Platform.pathSeparator}rss_feeds.json');
  }

  Future<List<String>> readUrls() async {
    try {
      final file = await _file();
      if (!await file.exists()) return const [];
      final txt = await file.readAsString();
      final data = jsonDecode(txt);
      if (data is! List) return const [];
      return data
          .map((e) => e.toString().trim())
          .where((e) => e.isNotEmpty)
          .toList(growable: false);
    } catch (_) {
      return const [];
    }
  }

  Future<void> writeUrls(List<String> urls) async {
    final normalized = urls
        .map((e) => e.trim())
        .where((e) => e.startsWith('http://') || e.startsWith('https://'))
        .toSet()
        .toList()
      ..sort();
    final file = await _file();
    await file.parent.create(recursive: true);
    await file.writeAsString(jsonEncode(normalized));
  }
}
