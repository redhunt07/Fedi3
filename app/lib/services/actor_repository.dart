/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:collection';
import 'dart:convert';

import 'package:http/http.dart' as http;

import '../core/core_http.dart';
import '../model/core_config.dart';

class ActorProfile {
  ActorProfile({
    required this.id,
    required this.preferredUsername,
    required this.name,
    required this.summary,
    required this.iconUrl,
    required this.imageUrl,
    required this.inbox,
    required this.outbox,
    required this.followers,
    required this.following,
    required this.url,
    required this.fields,
    required this.fedi3PeerId,
  });

  final String id;
  final String preferredUsername;
  final String name;
  final String summary;
  final String iconUrl;
  final String imageUrl;
  final String inbox;
  final String outbox;
  final String followers;
  final String following;
  final String url;
  final List<ProfileFieldKV> fields;
  final String fedi3PeerId;

  String get displayName => name.isNotEmpty ? name : preferredUsername;
  bool get isFedi3 => fedi3PeerId.isNotEmpty;

  String get statusKey {
    if (preferredUsername.isNotEmpty) return preferredUsername;
    final uri = Uri.tryParse(id);
    if (uri == null) return '';
    final segs = uri.pathSegments;
    if (segs.length >= 2 && segs.first == 'users') {
      return segs[1];
    }
    return '';
  }

  static ActorProfile? tryParse(Map<String, dynamic> json) {
    if ((json['type'] as String?) == null) return null;
    final id = (json['id'] as String?)?.trim() ?? '';
    if (id.isEmpty) return null;

    final preferredUsername =
        (json['preferredUsername'] as String?)?.trim() ?? '';
    final name = (json['name'] as String?)?.trim() ?? '';
    final summary = (json['summary'] as String?)?.trim() ?? '';
    final inbox = (json['inbox'] as String?)?.trim() ?? '';
    final outbox = (json['outbox'] as String?)?.trim() ?? '';
    final followers = (json['followers'] as String?)?.trim() ?? '';
    final following = (json['following'] as String?)?.trim() ?? '';
    final endpoints = json['endpoints'];
    var fedi3PeerId = '';
    if (endpoints is Map) {
      fedi3PeerId = (endpoints['fedi3PeerId'] as String?)?.trim() ?? '';
    }
    if (fedi3PeerId.isEmpty) {
      fedi3PeerId = (json['fedi3PeerId'] as String?)?.trim() ?? '';
    }

    String iconUrl = '';
    final icon = json['icon'];
    if (icon is Map) {
      final url = icon['url'];
      if (url is String) {
        iconUrl = url;
      } else if (url is Map) {
        iconUrl = (url['href'] as String?)?.trim() ?? '';
      }
    } else if (icon is List) {
      for (final v in icon) {
        if (v is! Map) continue;
        final url = v['url'];
        if (url is String && url.trim().isNotEmpty) {
          iconUrl = url.trim();
          break;
        }
      }
    }

    String imageUrl = '';
    final image = json['image'];
    if (image is Map) {
      final url = image['url'];
      if (url is String) {
        imageUrl = url;
      } else if (url is Map) {
        imageUrl = (url['href'] as String?)?.trim() ?? '';
      }
    } else if (image is List) {
      for (final v in image) {
        if (v is! Map) continue;
        final url = v['url'];
        if (url is String && url.trim().isNotEmpty) {
          imageUrl = url.trim();
          break;
        }
      }
    }

    String url = '';
    final urlValue = json['url'];
    if (urlValue is String) {
      url = urlValue.trim();
    } else if (urlValue is Map) {
      url = (urlValue['href'] as String?)?.trim() ?? '';
    } else if (urlValue is List) {
      for (final v in urlValue) {
        if (v is String && v.trim().isNotEmpty) {
          url = v.trim();
          break;
        }
        if (v is Map) {
          final href = (v['href'] as String?)?.trim() ?? '';
          if (href.isNotEmpty) {
            url = href;
            break;
          }
        }
      }
    }

    final fields = <ProfileFieldKV>[];
    final attachment = json['attachment'];
    if (attachment is List) {
      for (final v in attachment) {
        if (v is! Map) continue;
        final name = (v['name'] as String?)?.trim() ?? '';
        final value = (v['value'] as String?)?.trim() ?? '';
        if (name.isEmpty || value.isEmpty) continue;
        fields.add(ProfileFieldKV(name: name, value: value));
      }
    }

    return ActorProfile(
      id: id,
      preferredUsername: preferredUsername,
      name: name,
      summary: summary,
      iconUrl: iconUrl,
      imageUrl: imageUrl,
      inbox: inbox,
      outbox: outbox,
      followers: followers,
      following: following,
      url: url,
      fields: fields,
      fedi3PeerId: fedi3PeerId,
    );
  }
}

class OutboxPage {
  const OutboxPage({required this.items, required this.next});

  final List<Map<String, dynamic>> items;
  final String? next;
}

class ActorRepository {
  ActorRepository._();

  static final ActorRepository instance = ActorRepository._();

  http.Client get _client => CoreHttp.client;

  final LinkedHashMap<String, ActorProfile> _cache = LinkedHashMap();
  final LinkedHashMap<String, Future<ActorProfile?>> _inflight =
      LinkedHashMap();

  int maxCacheEntries = 512;

  Map<String, String> get _acceptHeaders => const {
        'Accept':
            'application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams", application/json',
      };

  Future<ActorProfile?> getActor(String actorUrl) async {
    final url = actorUrl.trim();
    if (url.isEmpty) return null;

    final cached = _cache[url];
    if (cached != null) {
      _cache.remove(url);
      _cache[url] = cached;
      return cached;
    }

    final existing = _inflight[url];
    if (existing != null) return existing;

    final fut = _fetchActor(url);
    _inflight[url] = fut;
    try {
      final profile = await fut;
      if (profile != null) _remember(url, profile);
      return profile;
    } finally {
      _inflight.remove(url);
    }
  }

  Future<ActorProfile?> refreshActor(String actorUrl) async {
    final url = actorUrl.trim();
    if (url.isEmpty) return null;
    final profile = await _fetchActor(url);
    if (profile != null) {
      _remember(url, profile);
    }
    return profile;
  }

  Future<ActorProfile?> _fetchActor(String url) async {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return null;
    try {
      final resp = await _client
          .get(uri, headers: _acceptHeaders)
          .timeout(CoreHttp.totalTimeout);
      if (resp.statusCode < 200 || resp.statusCode >= 300) return null;
      final json = jsonDecode(resp.body);
      if (json is! Map) return null;
      return ActorProfile.tryParse(json.cast<String, dynamic>());
    } catch (_) {
      // Actor lookups decorate already available content. An offline server or
      // an HTML error page must not raise an unhandled UI exception.
      return null;
    }
  }

  Future<List<Map<String, dynamic>>> fetchOutbox(String outboxUrl,
      {int limit = 20}) async {
    final page = await fetchOutboxPage(outboxUrl, limit: limit);
    return page.items;
  }

  /// Fetches one outbox page and returns the next page URL when present.
  /// Pass [pageUrl] to fetch a subsequent page.
  Future<OutboxPage> fetchOutboxPage(String outboxUrl,
      {int limit = 20, String? pageUrl}) async {
    Map<String, dynamic>? page;
    if (pageUrl != null && pageUrl.trim().isNotEmpty) {
      page = await _fetchJson(pageUrl.trim());
    } else {
      final first = await _fetchJson(outboxUrl);
      if (first == null) return const OutboxPage(items: [], next: null);
      page = first;
      final firstLink = first['first'];
      if (firstLink is String && firstLink.trim().isNotEmpty) {
        page = await _fetchJson(firstLink.trim());
      } else if (firstLink is Map) {
        final href = (firstLink['id'] as String?)?.trim() ??
            (firstLink['href'] as String?)?.trim() ??
            '';
        if (href.isNotEmpty) page = await _fetchJson(href);
      }
    }

    final items = (page?['orderedItems'] as List<dynamic>? ?? const []);
    final out = <Map<String, dynamic>>[];
    for (final it in items) {
      if (it is Map) out.add(it.cast<String, dynamic>());
      if (out.length >= limit) break;
    }

    String? next;
    final nextRaw = page?['next'];
    if (nextRaw is String && nextRaw.trim().isNotEmpty) {
      next = nextRaw.trim();
    } else if (nextRaw is Map) {
      final href = (nextRaw['id'] as String?)?.trim() ??
          (nextRaw['href'] as String?)?.trim();
      if (href != null && href.isNotEmpty) next = href;
    }
    return OutboxPage(items: out, next: next);
  }

  /// Best-effort local user search over the in-memory actor cache. The core
  /// search index only contains activity objects, so actors seen by the client
  /// (profiles, follow lists, threads) are matched here instead.
  List<ActorProfile> searchCached(String query, {int limit = 20}) {
    final needle = query.trim().toLowerCase();
    if (needle.isEmpty) return const [];
    final out = <ActorProfile>[];
    for (final p in _cache.values) {
      final hay = '${p.preferredUsername} ${p.displayName} ${p.id} ${p.url}'
          .toLowerCase();
      if (!hay.contains(needle)) continue;
      out.add(p);
      if (out.length >= limit) break;
    }
    return out;
  }

  /// Reads the actor URLs of a followers/following collection (first page).
  Future<List<String>> fetchCollectionItems(String collectionUrl,
      {int limit = 80, CoreConfig? config}) async {
    final first = await _fetchJson(_localCollectionUrl(collectionUrl, config));
    if (first == null) return const [];

    Map<String, dynamic>? page = first;
    final firstLink = first['first'];
    if (first['items'] is List || first['orderedItems'] is List) {
      page = first;
    } else if (firstLink is String && firstLink.trim().isNotEmpty) {
      page = await _fetchJson(_localCollectionUrl(firstLink.trim(), config));
    } else if (firstLink is Map &&
        (firstLink['items'] is List || firstLink['orderedItems'] is List)) {
      page = firstLink.cast<String, dynamic>();
    } else if (firstLink is Map) {
      final href = (firstLink['id'] as String?)?.trim() ??
          (firstLink['href'] as String?)?.trim() ??
          '';
      if (href.isNotEmpty) {
        page = await _fetchJson(_localCollectionUrl(href, config));
      }
    }

    final raw = (page?['orderedItems'] as List<dynamic>? ??
        page?['items'] as List<dynamic>? ??
        const <dynamic>[]);
    final out = <String>[];
    for (final it in raw) {
      String id = '';
      if (it is String) {
        id = it.trim();
      } else if (it is Map) {
        final v = it['id'] ?? it['actor'] ?? it['href'];
        if (v is String) id = v.trim();
      }
      if (id.isEmpty || out.contains(id)) continue;
      out.add(id);
      if (out.length >= limit) break;
    }
    return out;
  }

  Future<int?> fetchCollectionCount(String collectionUrl,
      {CoreConfig? config}) async {
    final data = await _fetchJson(_localCollectionUrl(collectionUrl, config));
    if (data == null) return null;
    final total = data['totalItems'];
    if (total is int) return total;
    if (total is num) return total.toInt();
    return null;
  }

  // Own relationship collections live in the local core database. Avoid a
  // round trip through the public relay tunnel just to read our own state.
  String _localCollectionUrl(String url, CoreConfig? config) {
    if (config == null) return url;
    final public = Uri.tryParse(config.publicBaseUrl);
    final uri = Uri.tryParse(url);
    if (public == null ||
        uri == null ||
        !const ['http', 'https'].contains(public.scheme) ||
        !const ['http', 'https'].contains(uri.scheme) ||
        public.host.isEmpty ||
        uri.host.isEmpty ||
        uri.origin != public.origin) {
      return url;
    }
    final basePath = public.path.replaceFirst(RegExp(r'/$'), '');
    final actor = '$basePath/users/${Uri.encodeComponent(config.username)}';
    if (uri.path != '$actor/followers' && uri.path != '$actor/following') {
      return url;
    }
    return config.localBaseUri
        .replace(path: uri.path.substring(basePath.length), query: uri.query)
        .toString();
  }

  Future<Map<String, dynamic>?> _fetchJson(String url) async {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return null;
    final resp = await _client
        .get(uri, headers: _acceptHeaders)
        .timeout(CoreHttp.totalTimeout);
    if (resp.statusCode < 200 || resp.statusCode >= 300) return null;
    final json = jsonDecode(resp.body);
    if (json is! Map) return null;
    return json.cast<String, dynamic>();
  }

  void _remember(String url, ActorProfile profile) {
    _cache[url] = profile;
    while (_cache.length > maxCacheEntries) {
      _cache.remove(_cache.keys.first);
    }
  }
}
