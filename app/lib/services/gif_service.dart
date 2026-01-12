/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:http/http.dart' as http;


class GifResult {
  GifResult({
    required this.id,
    required this.previewUrl,
    required this.originalUrl,
  });

  final String id;
  final String previewUrl;
  final String originalUrl;
}

class GifService {
  GifService._();

  static String _pickUrl(Map images, List<String> keys) {
    for (final key in keys) {
      final raw = images[key];
      if (raw is! Map) continue;
      final url = raw['url']?.toString() ?? raw['gif']?.toString() ?? '';
      if (url.isNotEmpty) return url;
    }
    return '';
  }

  static Future<List<GifResult>> search(
    String query, {
    int limit = 18,
    required String apiKey,
  }) async {
    final key = apiKey.trim();
    if (key.isEmpty) return const [];
    final q = query.trim();
    return _searchGiphy(q, limit, key);
  }

  static Future<List<GifResult>> _searchGiphy(String query, int limit, String apiKey) async {
    final giphy = query.isEmpty
        ? Uri.parse('https://api.giphy.com/v1/gifs/trending?api_key=$apiKey&limit=$limit')
        : Uri.parse('https://api.giphy.com/v1/gifs/search?api_key=$apiKey&q=${Uri.encodeQueryComponent(query)}&limit=$limit');
    final giphyResp = await http.get(giphy, headers: const {'User-Agent': 'Fedi3'});
    if (giphyResp.statusCode < 200 || giphyResp.statusCode >= 300) return const [];
    final json = jsonDecode(giphyResp.body);
    if (json is! Map) return const [];
    final data = json['data'];
    if (data is! List) return const [];
    final out = <GifResult>[];
    for (final it in data) {
      if (it is! Map) continue;
      final id = it['id']?.toString() ?? '';
      final images = it['images'];
      if (id.isEmpty || images is! Map) continue;
      final preview = _pickUrl(images, [
        'fixed_width_small',
        'fixed_width',
        'preview_gif',
        'downsized_small',
        'downsized',
        'original',
      ]);
      final originalUrl = _pickUrl(images, [
        'original',
        'downsized',
        'downsized_large',
        'fixed_width',
      ]);
      if (preview.isEmpty || originalUrl.isEmpty) continue;
      out.add(GifResult(id: id, previewUrl: preview, originalUrl: originalUrl));
    }
    return out;
  }

}
