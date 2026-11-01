/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:http/http.dart' as http;

import '../state/announcements_store.dart';

class AnnouncementEntry {
  const AnnouncementEntry({
    required this.id,
    required this.title,
    required this.date,
    required this.body,
  });

  final String id;
  final String title;
  final String date;
  final String body;
}

class AnnouncementsUpdate {
  const AnnouncementsUpdate({
    required this.entries,
    required this.hasNew,
    required this.newestId,
  });

  final List<AnnouncementEntry> entries;
  final bool hasNew;
  final String? newestId;
}

class AnnouncementsService {
  /// Raw Markdown maintained in the Fedi3 repository.
  static const String sourceUrl =
      'https://raw.githubusercontent.com/redhunt07/Fedi3/main/ANNOUNCEMENTS.md';

  static Future<AnnouncementsUpdate> checkForUpdates() async {
    final raw = await _fetchRaw();
    final entries = parse(raw);
    await AnnouncementsStore.cache(raw);
    final lastSeen = await AnnouncementsStore.lastSeenId();
    final newest = entries.isEmpty ? null : entries.first.id;
    final hasNew = newest != null && newest != lastSeen;
    return AnnouncementsUpdate(
      entries: entries,
      hasNew: hasNew,
      newestId: newest,
    );
  }

  static Future<List<AnnouncementEntry>> load({bool refresh = false}) async {
    if (!refresh) {
      final cached = await AnnouncementsStore.cached();
      if (cached != null && cached.trim().isNotEmpty) {
        final entries = parse(cached);
        if (entries.isNotEmpty) return entries;
      }
    }
    final raw = await _fetchRaw();
    await AnnouncementsStore.cache(raw);
    return parse(raw);
  }

  static Future<String> _fetchRaw() async {
    final resp = await http
        .get(Uri.parse(sourceUrl))
        .timeout(const Duration(seconds: 12));
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('announcements fetch failed: ${resp.statusCode}');
    }
    return utf8.decode(resp.bodyBytes);
  }

  static List<AnnouncementEntry> parse(String raw) {
    final lines = raw.replaceAll('\r\n', '\n').split('\n');
    final out = <AnnouncementEntry>[];
    String? title;
    String? date;
    final body = <String>[];

    void flush() {
      if (title == null || date == null) return;
      final text = body.join('\n').trim();
      out.add(AnnouncementEntry(
        id: '$date::$title',
        title: title!,
        date: date!,
        body: text,
      ));
      title = null;
      date = null;
      body.clear();
    }

    for (final line in lines) {
      if (line.startsWith('## ')) {
        flush();
        var heading = line.substring(3).trim();
        final dateMatch =
            RegExp(r'^(\d{4}-\d{2}-\d{2})\s*[—–-]\s*(.*)$').firstMatch(heading);
        if (dateMatch != null) {
          date = dateMatch.group(1);
          title = dateMatch.group(2)!.trim();
        } else {
          date = '';
          title = heading;
        }
      } else if (title != null) {
        body.add(line);
      }
    }
    flush();
    out.sort((a, b) => b.date.compareTo(a.date));
    return out;
  }
}
