/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:collection';
import 'dart:convert';

import 'package:http/http.dart' as http;

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
    required this.featured,
    required this.fields,
    required this.verifiedLinks,
    required this.aliases,
    required this.movedTo,
    required this.hasFedi3Did,
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
  final String featured;
  final List<ProfileFieldKV> fields;
  final List<String> verifiedLinks;
  final List<String> aliases;
  final String movedTo;
  final bool hasFedi3Did;

  String get displayName => name.isNotEmpty ? name : preferredUsername;
  bool get isFedi3 => hasFedi3Did;

  String get statusKey {
    if (!isFedi3) return '';
    if (preferredUsername.isNotEmpty) return preferredUsername.toLowerCase();
    final uri = Uri.tryParse(id);
    if (uri == null) return '';
    final segs = uri.pathSegments;
    if (segs.length >= 2 && segs.first == 'users') {
      return segs[1].toLowerCase();
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
    final inbox =
        _resolveMaybeRelative(id, (json['inbox'] as String?)?.trim() ?? '');
    final outbox =
        _resolveMaybeRelative(id, (json['outbox'] as String?)?.trim() ?? '');
    final followers =
        _resolveMaybeRelative(id, (json['followers'] as String?)?.trim() ?? '');
    final following =
        _resolveMaybeRelative(id, (json['following'] as String?)?.trim() ?? '');
    final featured =
        _resolveMaybeRelative(id, (json['featured'] as String?)?.trim() ?? '');
    var hasFedi3Did = false;
    final alsoKnownAsList = json['alsoKnownAs'];
    if (alsoKnownAsList is List) {
      for (final v in alsoKnownAsList) {
        if (v is String && v.trim().startsWith('did:fedi3:')) {
          hasFedi3Did = true;
          break;
        }
      }
    }
    if (!hasFedi3Did) {
      final did = (json['did'] as String?)?.trim() ?? '';
      if (did.startsWith('did:fedi3:')) {
        hasFedi3Did = true;
      }
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
    iconUrl = _resolveMaybeRelative(id, iconUrl);

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
    imageUrl = _resolveMaybeRelative(id, imageUrl);

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
      url = _resolveMaybeRelative(id, url);
    }

    final fields = <ProfileFieldKV>[];
    final verifiedLinks = <String>{};
    final attachment = json['attachment'];
    if (attachment is List) {
      for (final v in attachment) {
        if (v is! Map) continue;
        final name = (v['name'] as String?)?.trim() ?? '';
        final value = (v['value'] as String?)?.trim() ?? '';
        if (name.isEmpty || value.isEmpty) continue;
        fields.add(ProfileFieldKV(name: name, value: value));
        if (_hasRelMe(value)) {
          for (final href in _extractLinks(value)) {
            if (href.isNotEmpty) verifiedLinks.add(href);
          }
        }
      }
    }

    final aliases = <String>[];
    if (alsoKnownAsList is List) {
      for (final v in alsoKnownAsList) {
        if (v is String && v.trim().isNotEmpty) {
          aliases.add(v.trim());
        }
      }
    }
    final movedTo =
        _resolveMaybeRelative(id, (json['movedTo'] as String?)?.trim() ?? '');

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
      featured: featured,
      fields: fields,
      verifiedLinks: verifiedLinks.toList(),
      aliases: aliases,
      movedTo: movedTo,
      hasFedi3Did: hasFedi3Did,
    );
  }
}

String _resolveMaybeRelative(String baseUrl, String value) {
  final raw = value.trim();
  if (raw.isEmpty) return '';
  final uri = Uri.tryParse(raw);
  if (uri == null) return raw;
  if (uri.hasScheme && uri.host.isNotEmpty) return uri.toString();
  final base = Uri.tryParse(baseUrl.trim());
  if (base == null || base.host.isEmpty) return raw;
  if (raw.startsWith('//')) {
    return '${base.scheme}:$raw';
  }
  return base.resolve(raw).toString();
}

bool _hasRelMe(String html) {
  final relRe = RegExp('rel\\s*=\\s*([\'"]?)me\\1', caseSensitive: false);
  return relRe.hasMatch(html);
}

Iterable<String> _extractLinks(String html) sync* {
  final hrefRe =
      RegExp('href\\s*=\\s*([\'"])([^\'"]+)\\1', caseSensitive: false);
  for (final m in hrefRe.allMatches(html)) {
    final href = m.group(2)?.trim() ?? '';
    if (href.isNotEmpty) yield href;
  }
}

class ActorRepository {
  ActorRepository._();

  static final ActorRepository instance = ActorRepository._();

  final http.Client _client = http.Client();

  final LinkedHashMap<String, ActorProfile> _cache = LinkedHashMap();
  final LinkedHashMap<String, Future<ActorProfile?>> _inflight =
      LinkedHashMap();

  int maxCacheEntries = 512;
  Duration requestTimeout = const Duration(seconds: 8);

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
      final first = await _fetchJson(url);
      if (first != null) {
        final parsed = ActorProfile.tryParse(first);
        if (parsed != null) return parsed;
      }

      // Mastodon/Pleroma profile URLs like /@user are often HTML pages;
      // resolve to actor self URL through webfinger when direct fetch is non-AP.
      final actorSelf = await _resolveActorSelfUrlViaWebfinger(uri);
      if (actorSelf != null && actorSelf.trim().isNotEmpty) {
        final second = await _fetchJson(actorSelf);
        if (second != null) {
          return ActorProfile.tryParse(second);
        }
      }
      return null;
    } catch (_) {
      // Network/protocol/parse errors are non-fatal for UI.
      return null;
    }
  }

  Future<String?> _resolveActorSelfUrlViaWebfinger(Uri source) async {
    final host = source.host.trim();
    if (host.isEmpty) return null;
    final username = _extractUsernameFromProfilePath(source.pathSegments);
    if (username.isEmpty) return null;
    final wfUri = Uri(
      scheme: source.scheme.isEmpty ? 'https' : source.scheme,
      host: host,
      port: source.hasPort ? source.port : null,
      path: '/.well-known/webfinger',
      queryParameters: {'resource': 'acct:$username@$host'},
    );
    try {
      final resp = await _client.get(wfUri, headers: const {
        'Accept': 'application/jrd+json, application/json'
      }).timeout(requestTimeout);
      if (resp.statusCode < 200 || resp.statusCode >= 300) return null;
      final raw = jsonDecode(resp.body);
      if (raw is! Map) return null;
      final json = raw.cast<String, dynamic>();
      final links = json['links'];
      if (links is! List) return null;
      for (final link in links) {
        if (link is! Map) continue;
        final rel = (link['rel'] as String?)?.trim().toLowerCase() ?? '';
        final typ = (link['type'] as String?)?.trim().toLowerCase() ?? '';
        final href = (link['href'] as String?)?.trim() ?? '';
        if (href.isEmpty) continue;
        if (rel == 'self' &&
            (typ.contains('activity+json') ||
                typ.contains('activitystreams'))) {
          return href;
        }
      }
    } catch (_) {
      return null;
    }
    return null;
  }

  String _extractUsernameFromProfilePath(List<String> segments) {
    if (segments.isEmpty) return '';
    final first = segments.first.trim();
    if (first.startsWith('@') && first.length > 1) {
      return first.substring(1);
    }
    if (segments.length >= 2 && first.toLowerCase() == 'users') {
      final user = segments[1].trim();
      if (user.isNotEmpty) return user;
    }
    return '';
  }

  Future<List<Map<String, dynamic>>> fetchOutbox(String outboxUrl,
      {int limit = 20}) async {
    final page = await fetchOutboxPage(outboxUrl, limit: limit);
    return page.items;
  }

  Future<OutboxPage> fetchOutboxPage(String outboxUrl,
      {String? pageUrl, int limit = 20}) async {
    final root = await _fetchJson(outboxUrl);
    if (root == null) return const OutboxPage(items: [], next: null);

    Map<String, dynamic>? page = root;
    var pageBaseUrl = outboxUrl.trim();
    if (pageUrl != null && pageUrl.trim().isNotEmpty) {
      final resolved = _resolveCollectionLink(outboxUrl, pageUrl.trim());
      page = await _fetchJson(resolved);
      pageBaseUrl = resolved;
    } else {
      final firstLink = root['first'];
      if (firstLink is String && firstLink.trim().isNotEmpty) {
        final resolved = _resolveCollectionLink(outboxUrl, firstLink.trim());
        page = await _fetchJson(resolved);
        pageBaseUrl = resolved;
      } else if (firstLink is Map) {
        final href = (firstLink['id'] as String?)?.trim() ??
            (firstLink['href'] as String?)?.trim() ??
            '';
        if (href.isNotEmpty) {
          final resolved = _resolveCollectionLink(outboxUrl, href);
          page = await _fetchJson(resolved);
          pageBaseUrl = resolved;
        }
      }
    }
    page ??= root;
    final pageId = (page['id'] as String?)?.trim() ?? '';
    if (pageId.isNotEmpty) {
      pageBaseUrl = _resolveCollectionLink(pageBaseUrl, pageId);
    }

    final items = _readCollectionItems(page);
    final out = <Map<String, dynamic>>[];
    for (final it in items) {
      if (it is Map) {
        out.add(it.cast<String, dynamic>());
      } else if (it is String && it.trim().isNotEmpty) {
        final fetched = await _fetchJson(it.trim());
        if (fetched != null) out.add(fetched);
      }
      if (out.length >= limit) break;
    }

    String? next;
    final nextLink = page['next'];
    if (nextLink is String && nextLink.trim().isNotEmpty) {
      next = _resolveCollectionLink(pageBaseUrl, nextLink.trim());
    } else if (nextLink is Map) {
      final href = (nextLink['id'] as String?)?.trim() ??
          (nextLink['href'] as String?)?.trim() ??
          '';
      if (href.isNotEmpty) {
        next = _resolveCollectionLink(pageBaseUrl, href);
      }
    }

    return OutboxPage(items: out, next: next);
  }

  Future<List<dynamic>> fetchCollectionItems(String collectionUrl,
      {int limit = 20}) async {
    final page = await fetchCollectionPage(collectionUrl, limit: limit);
    return page.items;
  }

  Future<CollectionPage> fetchCollectionPage(
    String collectionUrl, {
    String? pageUrl,
    int limit = 20,
  }) async {
    final first = await _fetchJson(collectionUrl);
    if (first == null) return const CollectionPage(items: [], next: null);

    Map<String, dynamic>? page = first;
    var pageBaseUrl = collectionUrl.trim();
    if (pageUrl != null && pageUrl.trim().isNotEmpty) {
      final resolved = _resolveCollectionLink(collectionUrl, pageUrl.trim());
      page = await _fetchJson(resolved);
      pageBaseUrl = resolved;
    } else {
      final firstLink = first['first'];
      if (firstLink is String && firstLink.trim().isNotEmpty) {
        final resolved =
            _resolveCollectionLink(collectionUrl, firstLink.trim());
        page = await _fetchJson(resolved);
        pageBaseUrl = resolved;
      } else if (firstLink is Map) {
        final href = (firstLink['id'] as String?)?.trim() ??
            (firstLink['href'] as String?)?.trim() ??
            '';
        if (href.isNotEmpty) {
          final resolved = _resolveCollectionLink(collectionUrl, href);
          page = await _fetchJson(resolved);
          pageBaseUrl = resolved;
        }
      }
    }
    page ??= first;
    final pageId = (page['id'] as String?)?.trim() ?? '';
    if (pageId.isNotEmpty) {
      pageBaseUrl = _resolveCollectionLink(pageBaseUrl, pageId);
    }

    final items = _readCollectionItems(page);
    final out = items.take(limit).toList();
    String? next;
    final nextLink = page['next'];
    if (nextLink is String && nextLink.trim().isNotEmpty) {
      next = _resolveCollectionLink(pageBaseUrl, nextLink.trim());
    } else if (nextLink is Map) {
      final href = (nextLink['id'] as String?)?.trim() ??
          (nextLink['href'] as String?)?.trim() ??
          '';
      if (href.isNotEmpty) {
        next = _resolveCollectionLink(pageBaseUrl, href);
      }
    }
    return CollectionPage(items: out, next: next);
  }

  Future<int?> fetchCollectionCount(String collectionUrl) async {
    final data = await _fetchJson(collectionUrl);
    if (data == null) return null;
    final total = data['totalItems'];
    if (total is int) return total;
    if (total is num) return total.toInt();
    return null;
  }

  Future<Map<String, dynamic>?> _fetchJson(String url) async {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return null;
    try {
      final resp = await _client
          .get(uri, headers: _acceptHeaders)
          .timeout(requestTimeout);
      if (resp.statusCode < 200 || resp.statusCode >= 300) return null;
      final json = jsonDecode(resp.body);
      if (json is! Map) return null;
      return json.cast<String, dynamic>();
    } catch (_) {
      // Keep timeline/profile resilient on TLS or transient errors.
      return null;
    }
  }

  void _remember(String url, ActorProfile profile) {
    _cache[url] = profile;
    while (_cache.length > maxCacheEntries) {
      _cache.remove(_cache.keys.first);
    }
  }

  List<dynamic> _readCollectionItems(Map<String, dynamic>? page) {
    if (page == null) return const [];
    final ordered = page['orderedItems'];
    if (ordered is List && ordered.isNotEmpty) return ordered;
    final plain = page['items'];
    if (plain is List && plain.isNotEmpty) return plain;
    return const [];
  }

  String _resolveCollectionLink(String baseUrl, String value) {
    final raw = value.trim();
    if (raw.isEmpty) return raw;
    final parsed = Uri.tryParse(raw);
    if (parsed != null && parsed.hasScheme && parsed.host.isNotEmpty) {
      return parsed.toString();
    }
    final base = Uri.tryParse(baseUrl.trim());
    if (base == null || base.host.isEmpty) return raw;
    if (raw.startsWith('//')) {
      return '${base.scheme}:$raw';
    }
    return base.resolve(raw).toString();
  }
}

class OutboxPage {
  const OutboxPage({required this.items, required this.next});

  final List<Map<String, dynamic>> items;
  final String? next;
}

class CollectionPage {
  const CollectionPage({required this.items, required this.next});

  final List<dynamic> items;
  final String? next;
}
