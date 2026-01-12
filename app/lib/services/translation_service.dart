/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:http/http.dart' as http;

import '../model/ui_prefs.dart';

class TranslationResult {
  const TranslationResult({required this.text, required this.detectedSource});

  final String text;
  final String detectedSource;
}

class TranslationService {
  static final Map<String, TranslationResult> _cache = {};

  static Future<TranslationResult> translate({
    required String text,
    required String targetLang,
    required UiPrefs prefs,
  }) async {
    final trimmed = text.trim();
    if (trimmed.isEmpty) {
      throw const FormatException('empty text');
    }
    final cacheKey = _cacheKey(trimmed, targetLang, prefs);
    final cached = _cache[cacheKey];
    if (cached != null) return cached;
    final timeout = Duration(milliseconds: prefs.translationTimeoutMs);
    if (prefs.translationProvider == TranslationProvider.deeplx) {
      final res = await _translateDeepLx(trimmed, targetLang, prefs.translationDeepLxUrl, timeout);
      _cache[cacheKey] = res;
      return res;
    }
    final key = prefs.translationAuthKey.trim();
    if (key.isEmpty) {
      throw const FormatException('missing auth key');
    }
    final endpoint = prefs.translationUsePro ? 'api.deepl.com' : 'api-free.deepl.com';
    final uri = Uri.https(endpoint, '/v2/translate');
    final resp = await http
        .post(
          uri,
          headers: {
            'Authorization': 'DeepL-Auth-Key $key',
            'Content-Type': 'application/x-www-form-urlencoded',
          },
          body: {
            'text': trimmed,
            'target_lang': _normalizeLang(targetLang),
          },
        )
        .timeout(timeout);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('HTTP ${resp.statusCode}: ${resp.body}');
    }
    final json = jsonDecode(resp.body);
    if (json is! Map) throw const FormatException('invalid response');
    final list = json['translations'];
    if (list is List && list.isNotEmpty && list.first is Map) {
      final first = (list.first as Map).cast<String, dynamic>();
      final text = (first['text'] as String?)?.trim() ?? '';
      final src = (first['detected_source_language'] as String?)?.trim() ?? '';
      if (text.isEmpty) throw const FormatException('empty translation');
      final res = TranslationResult(text: text, detectedSource: src);
      _cache[cacheKey] = res;
      return res;
    }
    throw const FormatException('missing translations');
  }

  static Future<TranslationResult> _translateDeepLx(
    String text,
    String targetLang,
    String endpoint,
    Duration timeout,
  ) async {
    final url = endpoint.trim();
    if (url.isEmpty) throw const FormatException('missing deeplx endpoint');
    final uri = Uri.parse(url);
    final resp = await http
        .post(
          uri,
          headers: const {'Content-Type': 'application/json'},
          body: jsonEncode({
            'text': text,
            'target_lang': _normalizeLang(targetLang),
            'source_lang': 'auto',
          }),
        )
        .timeout(timeout);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('HTTP ${resp.statusCode}: ${resp.body}');
    }
    final json = jsonDecode(resp.body);
    if (json is Map) {
      final data = json['data'];
      if (data is String && data.trim().isNotEmpty) {
        final src = (json['source_lang'] as String?)?.trim() ?? '';
        return TranslationResult(text: data.trim(), detectedSource: src);
      }
      final translations = json['translations'];
      if (translations is List && translations.isNotEmpty && translations.first is Map) {
        final first = (translations.first as Map).cast<String, dynamic>();
        final text = (first['text'] as String?)?.trim() ?? '';
        final src = (first['detected_source_language'] as String?)?.trim() ?? '';
        if (text.isEmpty) throw const FormatException('empty translation');
        return TranslationResult(text: text, detectedSource: src);
      }
    }
    throw const FormatException('invalid deeplx response');
  }

  static String _cacheKey(String text, String targetLang, UiPrefs prefs) {
    final target = _normalizeLang(targetLang);
    final provider = prefs.translationProvider.name;
    final endpoint = prefs.translationProvider == TranslationProvider.deeplx ? prefs.translationDeepLxUrl.trim() : '';
    return '$provider|$endpoint|$target|${text.hashCode}';
  }

  static String _normalizeLang(String lang) {
    final l = lang.trim();
    if (l.isEmpty) return 'EN';
    return l.replaceAll('-', '_').split('_').first.toUpperCase();
  }
}
