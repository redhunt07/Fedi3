/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

/// Shared relay directory model.
///
/// The directory is a plain JSON file maintained in the Fedi3 repository
/// (`relays/relays.json`). Relays are added through pull requests, so clients
/// can bootstrap relay discovery without depending on any single relay being
/// reachable. Latency is measured locally and never stored in the file.
class RelayDirectory {
  RelayDirectory._();

  /// Canonical schema identifier accepted by the parser.
  static const String schema = 'fedi3-relay-directory/v1';

  /// Raw directory sources, tried in order. The jsDelivr mirror is a fallback
  /// for networks where raw.githubusercontent.com is blocked or slow.
  static const List<String> sourceUrls = [
    'https://fedi3.redhunt07.ovh/_fedi3/relay/directory',
    'https://raw.githubusercontent.com/redhunt07/Fedi3/main/relays/relays.json',
    'https://cdn.jsdelivr.net/gh/redhunt07/Fedi3@main/relays/relays.json',
  ];

  /// Human-readable page where users can submit their relay.
  static const String submitUrl =
      'https://github.com/redhunt07/Fedi3/blob/main/relays/README.md';
}

/// One relay advertised in the shared directory.
class RelayDirectoryEntry {
  const RelayDirectoryEntry({
    required this.url,
    required this.ws,
    required this.name,
    required this.operator,
    required this.contact,
    required this.region,
    required this.added,
  });

  /// Public HTTPS base URL without a trailing slash.
  final String url;

  /// WebSocket base URL without a trailing slash, inferred when absent.
  final String ws;

  final String name;
  final String operator;
  final String contact;
  final String? region;
  final String added;

  String get host => Uri.parse(url).host;

  /// Parses one directory item, returning null when the entry is malformed.
  /// A single bad entry must never break the whole list.
  static RelayDirectoryEntry? tryParse(Object? raw) {
    if (raw is! Map) return null;
    final url = _normalizeBase(raw['url'], secureOnly: true);
    if (url == null) return null;
    final explicitWs = raw['ws'];
    String ws;
    if (explicitWs is String && explicitWs.trim().isNotEmpty) {
      final parsed =
          _normalizeBase(explicitWs, secureOnly: true, websocket: true);
      if (parsed == null || Uri.parse(parsed).host != Uri.parse(url).host) {
        return null;
      }
      ws = parsed;
    } else {
      ws = _inferWs(url);
    }
    final name = _nonEmpty(raw['name']);
    final operator = _nonEmpty(raw['operator']);
    final contact = _nonEmpty(raw['contact']);
    final added = _nonEmpty(raw['added']);
    if (name == null || operator == null || contact == null || added == null) {
      return null;
    }
    return RelayDirectoryEntry(
      url: url,
      ws: ws,
      name: name,
      operator: operator,
      contact: contact,
      region: _nonEmpty(raw['region']),
      added: added,
    );
  }

  /// Serializes the entry back to the directory format.
  Map<String, dynamic> toJson() => {
        'url': url,
        'ws': ws,
        'name': name,
        'operator': operator,
        'contact': contact,
        if (region != null) 'region': region,
        'added': added,
      };

  @override
  String toString() => '$name ($host)';
}

/// A parsed directory document.
class RelayDirectorySnapshot {
  const RelayDirectorySnapshot({
    required this.entries,
    required this.sourceUrl,
    required this.fetchedAt,
    required this.updatedAt,
  });

  final List<RelayDirectoryEntry> entries;
  final String sourceUrl;
  final DateTime fetchedAt;

  /// `updated_at` from the directory file, when present.
  final String? updatedAt;
}

/// Raised when a directory document is structurally unusable.
class RelayDirectoryFormatException implements Exception {
  const RelayDirectoryFormatException(this.message);

  final String message;

  @override
  String toString() => 'RelayDirectoryFormatException: $message';
}

/// Parses a raw directory document.
///
/// Throws [RelayDirectoryFormatException] only when the document itself is
/// unusable (bad JSON, wrong schema, missing list). Malformed individual
/// entries are skipped.
List<RelayDirectoryEntry> parseRelayDirectory(String raw) {
  final decoded = _decode(raw);
  if (decoded['schema'] != RelayDirectory.schema) {
    throw RelayDirectoryFormatException(
        'unsupported relay directory schema: ${decoded['schema']}');
  }
  final relays = decoded['relays'];
  if (relays is! List) {
    throw const RelayDirectoryFormatException('relays must be an array');
  }
  final out = <RelayDirectoryEntry>[];
  final seen = <String>{};
  for (final item in relays) {
    final entry = RelayDirectoryEntry.tryParse(item);
    if (entry == null) continue;
    if (!seen.add(entry.url)) continue;
    out.add(entry);
  }
  return out;
}

/// Extracts the `updated_at` value from a raw directory document.
String? relayDirectoryUpdatedAt(String raw) {
  final decoded = _decode(raw);
  final value = decoded['updated_at'];
  return value is String && value.trim().isNotEmpty ? value.trim() : null;
}

Map<String, dynamic> _decode(String raw) {
  final Object? json;
  try {
    json = jsonDecode(raw);
  } on FormatException catch (e) {
    throw RelayDirectoryFormatException(
        'invalid relay directory JSON: ${e.message}');
  }
  if (json is! Map) {
    throw const RelayDirectoryFormatException(
        'relay directory must be a JSON object');
  }
  return json.cast<String, dynamic>();
}

String? _nonEmpty(Object? value) {
  if (value is! String) return null;
  final trimmed = value.trim();
  return trimmed.isEmpty ? null : trimmed;
}

String? _normalizeBase(Object? value,
    {required bool secureOnly, bool websocket = false}) {
  if (value is! String) return null;
  var text = value.trim();
  if (text.isEmpty || text.endsWith('/')) return null;
  final uri = Uri.tryParse(text);
  if (uri == null || uri.host.isEmpty) return null;
  final scheme = uri.scheme.toLowerCase();
  final allowed = websocket ? const {'ws', 'wss'} : const {'http', 'https'};
  if (!allowed.contains(scheme)) return null;
  if (secureOnly && scheme == 'http' && !_isLoopback(uri.host)) return null;
  if (secureOnly && scheme == 'ws' && !_isLoopback(uri.host)) return null;
  if (uri.hasQuery || uri.hasFragment) return null;
  return text;
}

bool _isLoopback(String host) =>
    host == 'localhost' ||
    host == '127.0.0.1' ||
    host == '::1' ||
    host == '[::1]';

String _inferWs(String base) {
  final uri = Uri.parse(base);
  return uri.replace(scheme: uri.scheme == 'https' ? 'wss' : 'ws').toString();
}
