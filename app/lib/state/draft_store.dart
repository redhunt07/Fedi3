/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:path_provider/path_provider.dart';

class ComposeDraft {
  const ComposeDraft({
    required this.text,
    required this.isPublic,
    required this.summary,
    required this.sensitive,
    required this.visibility,
    required this.directTo,
    required this.updatedAtMs,
  });

  final String text;
  final bool isPublic;
  final String summary;
  final bool sensitive;
  final String visibility;
  final String directTo;
  final int updatedAtMs;

  Map<String, dynamic> toJson() => {
        'text': text,
        'isPublic': isPublic,
        'summary': summary,
        'sensitive': sensitive,
        'visibility': visibility,
        'directTo': directTo,
        'updatedAtMs': updatedAtMs,
      };

  static ComposeDraft fromJson(Map<String, dynamic> json) {
    final isPublic = json['isPublic'] == true;
    final visibility = (json['visibility'] as String?)?.trim();
    return ComposeDraft(
      text: (json['text'] as String? ?? ''),
      isPublic: isPublic,
      summary: (json['summary'] as String? ?? ''),
      sensitive: json['sensitive'] == true,
      visibility: (visibility == null || visibility.isEmpty) ? (isPublic ? 'public' : 'followers') : visibility,
      directTo: (json['directTo'] as String? ?? ''),
      updatedAtMs: (json['updatedAtMs'] is num) ? (json['updatedAtMs'] as num).toInt() : 0,
    );
  }
}

class DraftStore {
  static Future<File> _file({required String username, required String domain}) async {
    String safe(String v) => v.trim().toLowerCase().replaceAll(RegExp(r'[^a-z0-9._-]+'), '_');
    final u = safe(username);
    final d = safe(domain);
    final dir = await getApplicationSupportDirectory();
    final name = 'compose_draft_${u}_$d.json';
    return File('${dir.path}${Platform.pathSeparator}$name');
  }

  static Future<ComposeDraft?> read({required String username, required String domain}) async {
    try {
      final f = await _file(username: username, domain: domain);
      if (!await f.exists()) return null;
      final txt = await f.readAsString();
      if (txt.trim().isEmpty) return null;
      final json = jsonDecode(txt);
      if (json is! Map) return null;
      return ComposeDraft.fromJson(json.cast<String, dynamic>());
    } catch (_) {
      return null;
    }
  }

  static Future<void> write({required String username, required String domain, required ComposeDraft draft}) async {
    final f = await _file(username: username, domain: domain);
    await f.parent.create(recursive: true);
    await f.writeAsString(jsonEncode(draft.toJson()));
  }

  static Future<void> clear({required String username, required String domain}) async {
    try {
      final f = await _file(username: username, domain: domain);
      if (await f.exists()) {
        await f.delete();
      }
    } catch (_) {}
  }
}
