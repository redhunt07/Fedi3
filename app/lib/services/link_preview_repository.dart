/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:collection';
import 'dart:convert';

import 'package:http/http.dart' as http;

class LinkPreview {
  LinkPreview({
    required this.url,
    required this.title,
    required this.description,
    required this.imageUrl,
    required this.siteName,
    this.playerUrl,
    this.youtubeVideoId,
  });

  final String url;
  final String title;
  final String description;
  final String imageUrl;
  final String siteName;
  final String? playerUrl;
  final String? youtubeVideoId;
}

class LinkPreviewRepository {
  LinkPreviewRepository._();

  static final LinkPreviewRepository instance = LinkPreviewRepository._();

  final http.Client _client = http.Client();

  final LinkedHashMap<String, LinkPreview> _cache = LinkedHashMap();
  final LinkedHashMap<String, Future<LinkPreview?>> _inflight = LinkedHashMap();
  final LinkedHashMap<String, int> _failUntil = LinkedHashMap();

  int maxCacheEntries = 256;
  int failTtlMs = 10 * 60 * 1000;

  Future<LinkPreview?> get(String url) async {
    final u = url.trim();
    if (u.isEmpty) return null;

    final cached = _cache[u];
    if (cached != null) {
      _cache.remove(u);
      _cache[u] = cached;
      return cached;
    }
    final now = DateTime.now().millisecondsSinceEpoch;
    final failUntil = _failUntil[u];
    if (failUntil != null && failUntil > now) {
      return null;
    }

    final existing = _inflight[u];
    if (existing != null) return existing;

    final fut = _fetch(u);
    _inflight[u] = fut;
    try {
      final p = await fut;
      if (p != null) {
        _remember(u, p);
        _failUntil.remove(u);
      } else {
        _failUntil[u] = DateTime.now().millisecondsSinceEpoch + failTtlMs;
        while (_failUntil.length > maxCacheEntries) {
          _failUntil.remove(_failUntil.keys.first);
        }
      }
      return p;
    } finally {
      _inflight.remove(u);
    }
  }

  Future<LinkPreview?> _fetch(String url) async {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return null;

    final ytId = _extractYoutubeId(uri);
    if (ytId != null && ytId.isNotEmpty) {
      final summary = await _fetchYouTubeOEmbed(url, ytId);
      return summary ?? _youtubeFallback(url, ytId);
    }

    http.Response resp;
    try {
      resp = await _client.get(
        uri,
        headers: const {
          'Accept': 'text/html,application/xhtml+xml',
          'User-Agent':
              'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0 Safari/537.36',
        },
      );
    } catch (_) {
      return null;
    }

    if (resp.statusCode < 200 || resp.statusCode >= 300) return null;
    final ct = (resp.headers['content-type'] ?? '').toLowerCase();
    if (!ct.contains('text/html') && !ct.contains('application/xhtml+xml')) return null;

    // Avoid keeping huge documents in memory.
    final bytes = resp.bodyBytes;
    const max = 256 * 1024;
    final clipped = bytes.length > max ? bytes.sublist(0, max) : bytes;
    final html = _decodeBestEffort(clipped);

    final title = _firstMeta(html, property: 'og:title') ??
        _firstMeta(html, name: 'twitter:title') ??
        _titleTag(html) ??
        '';
    final description = _firstMeta(html, property: 'og:description') ??
        _firstMeta(html, name: 'twitter:description') ??
        _firstMeta(html, name: 'description') ??
        '';
    final imageUrl = _firstMeta(html, property: 'og:image') ??
        _firstMeta(html, property: 'og:image:secure_url') ??
        _firstMeta(html, name: 'twitter:image') ??
        _firstLinkRel(html, rel: const ['apple-touch-icon', 'icon', 'shortcut icon']) ??
        '';
    final siteName = _firstMeta(html, property: 'og:site_name') ?? '';

    final resolvedTitle = _decodeHtmlEntities(title).trim();
    final resolvedDesc = _decodeHtmlEntities(description).trim();
    final resolvedSite = _decodeHtmlEntities(siteName).trim();
    final resolvedImage = _resolveMaybeRelative(uri, imageUrl.trim());

    if (resolvedTitle.isEmpty && resolvedDesc.isEmpty && resolvedImage.isEmpty) {
      return LinkPreview(
        url: url,
        title: uri.host,
        description: '',
        imageUrl: '',
        siteName: uri.host,
      );
    }
    return LinkPreview(
      url: url,
      title: resolvedTitle.isNotEmpty ? resolvedTitle : uri.host,
      description: resolvedDesc,
      imageUrl: resolvedImage,
      siteName: resolvedSite.isNotEmpty ? resolvedSite : uri.host,
    );
  }

  Future<LinkPreview?> _fetchYouTubeOEmbed(String url, String videoId) async {
    final apiUrl = Uri.parse(
      'https://www.youtube.com/oembed?url=${Uri.encodeComponent(url)}&format=json',
    );
    try {
      final resp = await _client.get(
        apiUrl,
        headers: const {
          'Accept': 'application/json, */*',
          'User-Agent': 'Fedi3/0.1 (+https://fedi3)',
        },
      );
      if (resp.statusCode < 200 || resp.statusCode >= 300) return null;
      final map = jsonDecode(resp.body);
      if (map is! Map) return null;
      final title = (map['title'] as String?)?.trim() ?? '';
      final author = (map['author_name'] as String?)?.trim() ?? '';
      final thumb = (map['thumbnail_url'] as String?)?.trim() ?? '';
      if (title.isEmpty && thumb.isEmpty) return null;
      return LinkPreview(
        url: url,
        title: title,
        description: author,
        imageUrl: thumb,
        siteName: 'YouTube',
        playerUrl: 'https://www.youtube.com/embed/$videoId',
        youtubeVideoId: videoId,
      );
    } catch (_) {
      return null;
    }
  }

  LinkPreview _youtubeFallback(String url, String videoId) {
    final img = 'https://i.ytimg.com/vi/$videoId/hqdefault.jpg';
    return LinkPreview(
      url: url,
      title: '',
      description: '',
      imageUrl: img,
      siteName: 'YouTube',
      playerUrl: 'https://www.youtube.com/embed/$videoId',
      youtubeVideoId: videoId,
    );
  }

  String? _extractYoutubeId(Uri uri) {
    final host = uri.host.toLowerCase();
    if (host == 'youtu.be' || host.endsWith('.youtu.be')) {
      final seg = uri.pathSegments;
      if (seg.isEmpty) return null;
      final id = seg.first.trim();
      return id.isEmpty ? null : id;
    }
    if (host.contains('youtube.com')) {
      final v = uri.queryParameters['v']?.trim();
      if (v != null && v.isNotEmpty) return v;
      // /shorts/<id>
      final seg = uri.pathSegments;
      final i = seg.indexOf('shorts');
      if (i >= 0 && i + 1 < seg.length) {
        final id = seg[i + 1].trim();
        if (id.isNotEmpty) return id;
      }
      // /v/<id>
      final vi = seg.indexOf('v');
      if (vi >= 0 && vi + 1 < seg.length) {
        final id = seg[vi + 1].trim();
        if (id.isNotEmpty) return id;
      }
      // /e/<id>
      final ei = seg.indexOf('e');
      if (ei >= 0 && ei + 1 < seg.length) {
        final id = seg[ei + 1].trim();
        if (id.isNotEmpty) return id;
      }
      // /embed/<id>
      final e = seg.indexOf('embed');
      if (e >= 0 && e + 1 < seg.length) {
        final id = seg[e + 1].trim();
        if (id.isNotEmpty) return id;
      }
    }
    return null;
  }

  static String _decodeBestEffort(List<int> bytes) {
    try {
      return utf8.decode(bytes, allowMalformed: true);
    } catch (_) {
      return latin1.decode(bytes, allowInvalid: true);
    }
  }

  static String? _titleTag(String html) {
    final m = RegExp(r'<title[^>]*>([^<]{1,300})</title>', caseSensitive: false).firstMatch(html);
    return m?.group(1);
  }

  static String? _firstMeta(String html, {String? property, String? name}) {
    final key = (property ?? name ?? '').trim();
    if (key.isEmpty) return null;
    final re = RegExp(
      property != null
          ? '(<meta[^>]+property=["\']${RegExp.escape(key)}["\'][^>]+>)'
          : '(<meta[^>]+name=["\']${RegExp.escape(key)}["\'][^>]+>)',
      caseSensitive: false,
    );
    final tag = re.firstMatch(html)?.group(1);
    if (tag == null) return null;
    final cm = RegExp("content=[\"']([^\"']{1,600})[\"']", caseSensitive: false).firstMatch(tag);
    return cm?.group(1);
  }

  static String? _firstLinkRel(String html, {required List<String> rel}) {
    for (final r in rel) {
      final re = RegExp(
        '(<link[^>]+rel=["\']${RegExp.escape(r)}["\'][^>]+>)',
        caseSensitive: false,
      );
      final tag = re.firstMatch(html)?.group(1);
      if (tag == null) continue;
      final href = RegExp('href=["\']([^"\']+)["\']', caseSensitive: false)
          .firstMatch(tag)
          ?.group(1);
      if (href != null && href.trim().isNotEmpty) return href.trim();
    }
    return null;
  }

  static String _resolveMaybeRelative(Uri base, String input) {
    final raw = input.trim();
    if (raw.isEmpty) return '';
    final uri = Uri.tryParse(raw);
    if (uri == null) return raw;
    if (uri.hasScheme) return uri.toString();
    if (raw.startsWith('//')) {
      return Uri.parse('${base.scheme}:$raw').toString();
    }
    return base.resolve(raw).toString();
  }

  static String _decodeHtmlEntities(String s) {
    // Minimal decoding for common entities found in meta tags.
    return s
        .replaceAll('&amp;', '&')
        .replaceAll('&quot;', '"')
        .replaceAll('&apos;', "'")
        .replaceAll('&#39;', "'")
        .replaceAll('&#039;', "'")
        .replaceAll('&amp;#039;', "'")
        .replaceAll('&lt;', '<')
        .replaceAll('&gt;', '>');
  }

  void _remember(String url, LinkPreview preview) {
    _cache[url] = preview;
    while (_cache.length > maxCacheEntries) {
      _cache.remove(_cache.keys.first);
    }
  }
}
