/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:http/http.dart' as http;
import 'package:mime/mime.dart';

import '../model/chat_models.dart';
import '../model/core_config.dart';
import '../model/follow_request.dart';
import 'core_http.dart';

/// Classification of every failure the app can get from the core or the relay.
///
/// AN-01..10: the UI must be able to tell a timeout from an auth problem and a
/// server error, and decide whether a retry makes sense.
enum CoreApiErrorKind {
  timeout,
  network,
  unauthorized,
  forbidden,
  notFound,
  badRequest,
  server,
  decode,
  invalidConfig,
}

class CoreApiException implements Exception {
  CoreApiException(
    this.kind,
    this.message, {
    this.statusCode,
    this.retryable = false,
    this.cause,
  });

  final CoreApiErrorKind kind;
  final String message;
  final int? statusCode;
  final bool retryable;
  final Object? cause;

  bool get isTimeout => kind == CoreApiErrorKind.timeout;

  bool get isAuth =>
      kind == CoreApiErrorKind.unauthorized ||
      kind == CoreApiErrorKind.forbidden;

  factory CoreApiException.invalidConfig(String message, {Object? cause}) =>
      CoreApiException(CoreApiErrorKind.invalidConfig, message, cause: cause);

  @override
  String toString() {
    final causeText = cause == null ? '' : ' ($cause)';
    return 'CoreApiException(${kind.name}'
        '${statusCode == null ? '' : ', status=$statusCode'}): $message$causeText';
  }
}

class CoreApi {
  CoreApi({
    required this.config,
    http.Client? client,
    Duration? totalTimeout,
  })  : _client = client,
        _totalTimeout = totalTimeout ?? CoreHttp.totalTimeout;

  final CoreConfig config;
  final http.Client? _client;
  final Duration _totalTimeout;

  http.Client get _http => _client ?? CoreHttp.client;

  Map<String, String> get _internalHeaders => {
        if (config.internalToken.trim().isNotEmpty)
          'X-Fedi3-Internal': config.internalToken.trim(),
      };

  Map<String, String> get _jsonHeaders => {
        ..._internalHeaders,
        'Content-Type': 'application/json',
      };

  Map<String, String> get _activityHeaders => {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      };

  Map<String, String> get _bearerHeaders => {
        if (config.relayToken.trim().isNotEmpty)
          'Authorization': 'Bearer ${config.relayToken.trim()}',
      };

  // ---------------------------------------------------------------------------
  // URL construction (M-33: never let Uri.parse throw an unhandled
  // FormatException on an empty or malformed configuration).
  // ---------------------------------------------------------------------------

  Uri get _localBaseUri {
    final bind = config.bind.trim();
    if (bind.isEmpty) {
      throw CoreApiException.invalidConfig('core bind address is empty');
    }
    try {
      return Uri.parse('http://$bind');
    } on FormatException catch (e) {
      throw CoreApiException.invalidConfig('invalid core bind address: $bind',
          cause: e);
    }
  }

  Uri get _relayBaseUri {
    final raw = config.publicBaseUrl.trim();
    if (raw.isEmpty) {
      throw CoreApiException.invalidConfig('publicBaseUrl is empty');
    }
    Uri parsed;
    try {
      parsed = Uri.parse(raw);
    } on FormatException catch (e) {
      throw CoreApiException.invalidConfig('invalid publicBaseUrl: $raw',
          cause: e);
    }
    if (parsed.host.isEmpty ||
        (parsed.scheme != 'http' && parsed.scheme != 'https')) {
      throw CoreApiException.invalidConfig(
          'publicBaseUrl must be an absolute http(s) URL: $raw');
    }
    return parsed;
  }

  bool get _bindLooksLocal {
    try {
      final host = _localBaseUri.host.toLowerCase();
      return host == '127.0.0.1' ||
          host == 'localhost' ||
          host == '0.0.0.0' ||
          host == '::1';
    } on CoreApiException {
      return true;
    }
  }

  Uri _baseLocalOrRelay() => _bindLooksLocal ? _localBaseUri : _relayBaseUri;

  Uri _internal(String path, [Map<String, String>? query]) {
    return _localBaseUri.replace(path: path, queryParameters: query);
  }

  Uri _localOrRelay(String path, [Map<String, String>? query]) {
    return _baseLocalOrRelay().replace(path: path, queryParameters: query);
  }

  Uri _relay(String path, [Map<String, String>? query]) {
    return _relayBaseUri.replace(path: path, queryParameters: query);
  }

  String get _publicBase =>
      config.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '');

  String get _me =>
      '$_publicBase/users/${Uri.encodeComponent(config.username.trim())}';

  // ---------------------------------------------------------------------------
  // Single request helper (D1/H-50/AN-01..10).
  // ---------------------------------------------------------------------------

  Future<http.Response> _request({
    required String method,
    required Uri uri,
    Map<String, String>? headers,
    Object? body,
    Set<int> okStatus = const {200},
    bool anySuccess = false,
  }) async {
    final request = http.Request(method, uri);
    if (headers != null) request.headers.addAll(headers);
    if (body != null) {
      if (body is String) {
        request.body = body;
      } else if (body is List<int>) {
        request.bodyBytes = body;
      } else if (body is Uint8List) {
        request.bodyBytes = body;
      } else {
        request.body = jsonEncode(body);
      }
    }

    http.Response resp;
    try {
      final streamed = await _http.send(request).timeout(_totalTimeout);
      resp = await http.Response.fromStream(streamed).timeout(_totalTimeout);
    } on CoreApiException {
      rethrow;
    } on TimeoutException catch (e) {
      throw CoreApiException(
        CoreApiErrorKind.timeout,
        '$method $uri timed out after ${_totalTimeout.inSeconds}s',
        retryable: true,
        cause: e,
      );
    } on SocketException catch (e) {
      throw CoreApiException(
        CoreApiErrorKind.network,
        '$method $uri failed: $e',
        retryable: true,
        cause: e,
      );
    } on http.ClientException catch (e) {
      throw CoreApiException(
        CoreApiErrorKind.network,
        '$method $uri failed: $e',
        retryable: true,
        cause: e,
      );
    } on IOException catch (e) {
      throw CoreApiException(
        CoreApiErrorKind.network,
        '$method $uri failed: $e',
        retryable: true,
        cause: e,
      );
    } catch (e) {
      throw CoreApiException(
        CoreApiErrorKind.network,
        '$method $uri failed: $e',
        retryable: true,
        cause: e,
      );
    }

    final status = resp.statusCode;
    if (okStatus.contains(status) ||
        (anySuccess && status >= 200 && status < 300)) {
      return resp;
    }
    throw _statusException(method, uri, resp);
  }

  CoreApiException _statusException(
      String method, Uri uri, http.Response resp) {
    final status = resp.statusCode;
    final snippet = _bodySnippet(resp.body);
    final detail =
        '$method $uri returned $status${snippet.isEmpty ? '' : ': $snippet'}';
    if (status == 401) {
      return CoreApiException(CoreApiErrorKind.unauthorized, detail,
          statusCode: status);
    }
    if (status == 403) {
      return CoreApiException(CoreApiErrorKind.forbidden, detail,
          statusCode: status);
    }
    if (status == 404) {
      return CoreApiException(CoreApiErrorKind.notFound, detail,
          statusCode: status);
    }
    if (status == 400 || status == 409 || status == 413 || status == 422) {
      return CoreApiException(CoreApiErrorKind.badRequest, detail,
          statusCode: status);
    }
    if (status >= 500) {
      return CoreApiException(
        CoreApiErrorKind.server,
        detail,
        statusCode: status,
        retryable: true,
      );
    }
    return CoreApiException(CoreApiErrorKind.server, detail,
        statusCode: status);
  }

  String _bodySnippet(String body) {
    final trimmed = body.trim();
    if (trimmed.isEmpty) return '';
    return trimmed.length <= 512 ? trimmed : '${trimmed.substring(0, 512)}…';
  }

  Map<String, dynamic> _decodeMap(http.Response resp, String operation) {
    try {
      final json = jsonDecode(resp.body);
      if (json is Map) return json.cast<String, dynamic>();
      throw const FormatException('expected a JSON object');
    } on FormatException catch (e) {
      throw CoreApiException(
        CoreApiErrorKind.decode,
        '$operation returned invalid JSON',
        cause: e,
      );
    }
  }

  /// Percent-encodes a header value when it contains non-ASCII bytes.
  ///
  /// H-66: `dart:io` rejects header values with non-ASCII bytes, so uploading
  /// `città.jpg` used to fail with an opaque `HttpException`.
  static String encodeHeaderValue(String value) {
    final asciiPrintable = RegExp(r'^[\x20-\x7E]*$');
    if (asciiPrintable.hasMatch(value)) return value;
    return Uri.encodeComponent(value);
  }

  // ---------------------------------------------------------------------------
  // Health (C12/H-51 watchdog).
  // ---------------------------------------------------------------------------

  /// GET /_fedi3/health. Throws [CoreApiException] on failure.
  Future<Map<String, dynamic>> health() async {
    final resp = await _request(
      method: 'GET',
      uri: _internal('/_fedi3/health'),
      headers: _internalHeaders,
    );
    return _decodeMap(resp, 'health');
  }

  /// Non-throwing health probe used by the watchdog.
  Future<bool> pingHealth() async {
    try {
      await health();
      return true;
    } on CoreApiException {
      return false;
    }
  }

  // ---------------------------------------------------------------------------
  // Actors / webfinger.
  // ---------------------------------------------------------------------------

  Future<String> _resolveActorInput(String input) async {
    var v = input.trim();
    if (v.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'missing actor');
    }

    var user = '';
    var host = '';

    if (v.startsWith('http://') || v.startsWith('https://')) {
      final uri = Uri.tryParse(v);
      if (uri == null || uri.host.isEmpty) {
        throw CoreApiException(
            CoreApiErrorKind.badRequest, 'invalid actor URL: $v');
      }
      host = uri.host;

      final path = uri.path;
      if (path.startsWith('/@')) {
        user = path.substring(2).split('/').first;
        if (user.contains('@')) user = user.split('@').first;
      } else if (path.startsWith('/users/')) {
        return v.replaceAll(RegExp(r'/+$'), '');
      } else {
        return v.replaceAll(RegExp(r'/+$'), '');
      }
    } else if (v.contains('@')) {
      v = v.startsWith('@') ? v.substring(1) : v;
      final parts = v.split('@').where((p) => p.trim().isNotEmpty).toList();
      if (parts.length < 2) {
        throw CoreApiException(
            CoreApiErrorKind.badRequest, 'invalid handle: $v');
      }
      user = parts.first;
      host = parts.last;
    } else {
      throw CoreApiException(
          CoreApiErrorKind.badRequest, 'unsupported actor format: $v');
    }

    if (user.isEmpty || host.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'invalid actor: $v');
    }

    final wf = Uri.parse('https://$host/.well-known/webfinger')
        .replace(queryParameters: {'resource': 'acct:$user@$host'});

    final resp = await _request(
      method: 'GET',
      uri: wf,
      headers: {'Accept': 'application/jrd+json, application/json'},
    );

    Map<String, dynamic> json;
    try {
      final decoded = jsonDecode(resp.body);
      if (decoded is! Map) {
        throw const FormatException('expected a JSON object');
      }
      json = decoded.cast<String, dynamic>();
    } on FormatException catch (e) {
      throw CoreApiException(
          CoreApiErrorKind.decode, 'webfinger returned invalid JSON',
          cause: e);
    }

    final links = (json['links'] as List<dynamic>? ?? const []);
    for (final link in links) {
      if (link is! Map) continue;
      final rel = link['rel']?.toString();
      final href = link['href']?.toString();
      final type = link['type']?.toString() ?? '';
      if (rel == 'self' && href != null && href.isNotEmpty) {
        if (type.contains('activity+json') ||
            type.contains('application/json') ||
            type.isEmpty) {
          return href.replaceAll(RegExp(r'/+$'), '');
        }
      }
    }
    for (final link in links) {
      if (link is! Map) continue;
      final href = link['href']?.toString();
      if (href != null && href.isNotEmpty) {
        return href.replaceAll(RegExp(r'/+$'), '');
      }
    }
    throw CoreApiException(
        CoreApiErrorKind.decode, 'webfinger: no actor link for $v');
  }

  Future<String> resolveActorInput(String input) async {
    return _resolveActorInput(input);
  }

  // ---------------------------------------------------------------------------
  // Timelines, objects, search, notifications.
  // ---------------------------------------------------------------------------

  Future<Map<String, dynamic>> fetchTimeline(String kind,
      {String? cursor, int limit = 50}) async {
    final uri = _internal('/_fedi3/timeline/$kind', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'timeline $kind');
  }

  Future<void> triggerLegacySync(
      {int pages = 6, int itemsPerActor = 200}) async {
    final uri = _internal('/_fedi3/sync/legacy', {
      'pages': pages.toString(),
      'items': itemsPerActor.toString(),
    });
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: '{}',
      anySuccess: true,
    );
  }

  Future<Map<String, dynamic>?> fetchCachedObject(String url) async {
    final u = url.trim();
    if (u.isEmpty) return null;
    final uri = _internal('/_fedi3/object', {'url': u});
    try {
      final resp = await _request(
        method: 'GET',
        uri: uri,
        headers: _internalHeaders,
        okStatus: const {200, 404},
      );
      if (resp.statusCode == 404) return null;
      final json = jsonDecode(resp.body);
      if (json is! Map) return null;
      return json.cast<String, dynamic>();
    } on FormatException catch (e) {
      throw CoreApiException(
          CoreApiErrorKind.decode, 'object fetch returned invalid JSON',
          cause: e);
    }
  }

  Future<Map<String, dynamic>> fetchNoteReplies(String noteId,
      {String? cursor, int limit = 20}) async {
    final uri = _internal('/_fedi3/note/replies', {
      'note': noteId,
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'note replies');
  }

  Future<Map<String, dynamic>> fetchReactions(String objectId,
      {int limit = 50}) async {
    final uri = _internal('/_fedi3/reactions', {
      'object': objectId,
      'limit': limit.toString(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'reactions');
  }

  Future<Map<String, dynamic>> fetchMyReactions(String objectId) async {
    final uri = _internal('/_fedi3/reactions/me', {'object': objectId});
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'my reactions');
  }

  Future<List<String>> fetchReactionActors({
    required String objectId,
    required String type,
    String? content,
    int limit = 50,
  }) async {
    final uri = _internal('/_fedi3/reactions/actors', {
      'object': objectId,
      'type': type,
      if (content != null && content.trim().isNotEmpty)
        'content': content.trim(),
      'limit': limit.toString(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    try {
      final json = jsonDecode(resp.body);
      if (json is! Map) return const [];
      final items = json['items'];
      if (items is! List) return const [];
      return items
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList();
    } on FormatException catch (e) {
      throw CoreApiException(
          CoreApiErrorKind.decode, 'reaction actors returned invalid JSON',
          cause: e);
    }
  }

  Future<Map<String, dynamic>> searchNotes({
    required String query,
    String? tag,
    String? cursor,
    String? source,
    String? consistency,
    int limit = 30,
  }) async {
    final params = <String, String>{
      'q': query,
      if (tag != null && tag.trim().isNotEmpty) 'tag': tag.trim(),
      'limit': limit.toString(),
      if (source != null && source.trim().isNotEmpty) 'source': source.trim(),
      if (consistency != null && consistency.trim().isNotEmpty)
        'consistency': consistency.trim(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    };
    final uri = _internal('/_fedi3/search/notes', params);
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'search notes');
  }

  Future<Map<String, dynamic>> searchUsers({
    required String query,
    String? cursor,
    String? source,
    String? consistency,
    int limit = 30,
  }) async {
    final params = <String, String>{
      'q': query,
      'limit': limit.toString(),
      if (source != null && source.trim().isNotEmpty) 'source': source.trim(),
      if (consistency != null && consistency.trim().isNotEmpty)
        'consistency': consistency.trim(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    };
    final uri = _internal('/_fedi3/search/users', params);
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'search users');
  }

  Future<Map<String, dynamic>> fetchSuggestedActors({int limit = 6}) async {
    final uri = _internal('/_fedi3/suggested/actors', {
      'limit': limit.toString(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'suggested actors');
  }

  Future<Map<String, dynamic>> fetchTrendingHashtags({int limit = 10}) async {
    final uri = _internal('/_fedi3/trending/hashtags', {
      'limit': limit.toString(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'trending hashtags');
  }

  Future<Map<String, dynamic>> searchHashtags({
    required String query,
    String? source,
    String? consistency,
    int limit = 30,
  }) async {
    final params = <String, String>{
      'q': query,
      'limit': limit.toString(),
      if (source != null && source.trim().isNotEmpty) 'source': source.trim(),
      if (consistency != null && consistency.trim().isNotEmpty)
        'consistency': consistency.trim(),
    };
    final uri = _internal('/_fedi3/search/hashtags', params);
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'search hashtags');
  }

  Future<Map<String, dynamic>> fetchNotifications(
      {String? cursor, int limit = 50}) async {
    final uri = _internal('/_fedi3/notifications', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'notifications');
  }

  Future<Map<String, dynamic>> fetchNetMetrics() async {
    final uri = _internal('/_fedi3/net/metrics');
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'net metrics');
  }

  Future<void> refreshProfile() async {
    final uri = _internal('/_fedi3/profile/refresh');
    await _request(
        method: 'POST', uri: uri, headers: _internalHeaders, anySuccess: true);
  }

  // ---------------------------------------------------------------------------
  // Chat. Thread ids are opaque and may contain `/` or `%` (AN-07/H-66).
  // ---------------------------------------------------------------------------

  Future<Map<String, dynamic>> fetchChatThreads(
      {String? cursor, int limit = 50}) async {
    final uri = _internal('/_fedi3/chat/threads', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'chat threads');
  }

  Future<Map<String, dynamic>> fetchChatMembers(
      {required String threadId}) async {
    final uri =
        _internal('/_fedi3/chat/thread/members', {'thread_id': threadId});
    final resp = await _request(
      method: 'GET',
      uri: uri,
      headers: _internalHeaders,
      okStatus: const {200, 404},
    );
    if (resp.statusCode == 404) return {'items': const []};
    return _decodeMap(resp, 'chat members');
  }

  Future<List<ChatSearchHit>> searchAllChatMessages({
    required String query,
    int limit = 50,
  }) async {
    final uri = _internal('/_fedi3/chat/search', {
      'q': query,
      'limit': '$limit',
    });
    final resp = await _request(
      method: 'GET',
      uri: uri,
      headers: _internalHeaders,
      okStatus: const {200, 404},
    );
    if (resp.statusCode == 404) return const [];
    final map = _decodeMap(resp, 'chat global search');
    final raw = map['items'];
    if (raw is! List) return const [];
    return raw
        .whereType<Map>()
        .map((m) => ChatSearchHit.fromJson(m.cast<String, dynamic>()))
        .where((h) => h.threadId.isNotEmpty)
        .toList(growable: false);
  }

  Future<void> sendChatCallSignal({
    required String threadId,
    required String callId,
    required String action,
    String? to,
    Map<String, dynamic>? data,
  }) async {
    final uri = _internal('/_fedi3/chat/call/signal');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({
        'thread_id': threadId,
        'call_id': callId,
        'action': action,
        if (to != null && to.isNotEmpty) 'to': to,
        if (data != null) 'data': data,
      }),
      anySuccess: true,
    );
  }

  Future<List<ChatCallSignal>> fetchGlobalChatCallSignals({
    required int afterId,
    required int afterMs,
    int limit = 100,
  }) async {
    final uri = _internal('/_fedi3/chat/call/signals', {
      'after_id': afterId.toString(),
      'after_ms': '$afterMs',
      'limit': '$limit',
    });
    final resp = await _request(
      method: 'GET',
      uri: uri,
      headers: _internalHeaders,
      okStatus: const {200, 404},
    );
    if (resp.statusCode == 404) return const [];
    final map = _decodeMap(resp, 'chat call signals');
    final raw = map['items'];
    if (raw is! List) return const [];
    return raw
        .whereType<Map>()
        .map((m) => ChatCallSignal.fromJson(m.cast<String, dynamic>()))
        .toList(growable: false);
  }

  Future<List<ChatCallSignal>> fetchChatCallSignals({
    required String threadId,
    required int afterMs,
    int afterId = 0,
    int limit = 100,
  }) async {
    final uri = _internal('/_fedi3/chat/call/signals', {
      'thread_id': threadId,
      if (afterId > 0) 'after_id': '$afterId',
      'after_ms': '$afterMs',
      'limit': '$limit',
    });
    final resp = await _request(
      method: 'GET',
      uri: uri,
      headers: _internalHeaders,
      okStatus: const {200, 404},
    );
    if (resp.statusCode == 404) return const [];
    final map = _decodeMap(resp, 'chat call signals');
    final raw = map['items'];
    if (raw is! List) return const [];
    return raw
        .whereType<Map>()
        .map((m) => ChatCallSignal.fromJson(m.cast<String, dynamic>()))
        .toList(growable: false);
  }

  Future<Map<String, dynamic>> searchChatMessages({
    required String threadId,
    required String query,
    int limit = 50,
  }) async {
    final uri = _internal('/_fedi3/chat/thread/search', {
      'thread_id': threadId,
      'q': query,
      'limit': '$limit',
    });
    final resp = await _request(
      method: 'GET',
      uri: uri,
      headers: _internalHeaders,
      okStatus: const {200, 404},
    );
    if (resp.statusCode == 404) return {'items': const []};
    return _decodeMap(resp, 'chat search');
  }

  Future<Map<String, dynamic>?> fetchChatPin({required String threadId}) async {
    final uri = _internal('/_fedi3/chat/thread/pin', {'thread_id': threadId});
    final resp = await _request(
      method: 'GET',
      uri: uri,
      headers: _internalHeaders,
      okStatus: const {200, 404},
    );
    if (resp.statusCode == 404) return null;
    final map = _decodeMap(resp, 'chat pin');
    final pin = map['pin'];
    return pin is Map ? pin.cast<String, dynamic>() : null;
  }

  Future<void> setChatPin({
    required String threadId,
    required String messageId,
    required bool pinned,
  }) async {
    final uri = _internal('/_fedi3/chat/thread/pin');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({
        'thread_id': threadId,
        'message_id': messageId,
        'pinned': pinned,
      }),
      anySuccess: true,
    );
  }

  Future<Map<String, dynamic>> joinChatThread({
    required String threadId,
    String? title,
  }) async {
    final uri = _internal('/_fedi3/chat/thread/join');
    final resp = await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({
        'thread_id': threadId,
        if (title != null && title.trim().isNotEmpty) 'title': title.trim(),
      }),
      anySuccess: true,
    );
    return _decodeMap(resp, 'join chat');
  }

  Future<Map<String, dynamic>> fetchChatMessages({
    required String threadId,
    String? cursor,
    int limit = 50,
  }) async {
    final uri =
        _internal('/_fedi3/chat/threads/${Uri.encodeComponent(threadId)}', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'chat messages');
  }

  Future<Map<String, dynamic>> createChatThread({
    required List<String> recipients,
    String? title,
    String kind = 'dm',
  }) async {
    final uri = _internal('/_fedi3/chat/thread/create');
    final resp = await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({
        'recipients': recipients,
        if (title != null && title.trim().isNotEmpty) 'title': title.trim(),
        'kind': kind,
      }),
    );
    return _decodeMap(resp, 'chat thread create');
  }

  Future<void> setChatMemberRole({
    required String threadId,
    required String actor,
    required String role,
  }) async {
    final uri = _internal('/_fedi3/chat/thread/members');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({
        'thread_id': threadId,
        'roles': {actor: role},
      }),
      anySuccess: true,
    );
  }

  Future<Map<String, dynamic>> sendChatMessage({
    String? threadId,
    required List<String> recipients,
    required String text,
    String? replyTo,
    String? title,
    List<Map<String, dynamic>>? attachments,
  }) async {
    final uri = _internal('/_fedi3/chat/send');
    final body = jsonEncode({
      if (threadId != null && threadId.trim().isNotEmpty)
        'thread_id': threadId.trim(),
      'recipients': recipients,
      'text': text,
      if (replyTo != null && replyTo.trim().isNotEmpty)
        'reply_to': replyTo.trim(),
      if (title != null && title.trim().isNotEmpty) 'title': title.trim(),
      if (attachments != null && attachments.isNotEmpty)
        'attachments': attachments,
    });
    final resp = await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: body,
      anySuccess: true,
    );
    return _decodeMap(resp, 'chat send');
  }

  Future<void> updateChatThreadTitle(
      {required String threadId, required String title}) async {
    final uri = _internal('/_fedi3/chat/thread/update');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'thread_id': threadId, 'title': title}),
      anySuccess: true,
    );
  }

  Future<void> deleteChatThread({required String threadId}) async {
    final uri = _internal('/_fedi3/chat/thread/delete');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'thread_id': threadId}),
      anySuccess: true,
    );
  }

  Future<void> leaveChatThread({required String threadId}) async {
    final uri = _internal('/_fedi3/chat/thread/leave');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'thread_id': threadId}),
      anySuccess: true,
    );
  }

  Future<void> updateChatThreadMembers({
    required String threadId,
    List<String>? add,
    List<String>? remove,
  }) async {
    final uri = _internal('/_fedi3/chat/thread/members');
    final body = jsonEncode({
      'thread_id': threadId,
      if (add != null && add.isNotEmpty) 'add': add,
      if (remove != null && remove.isNotEmpty) 'remove': remove,
    });
    await _request(
        method: 'POST',
        uri: uri,
        headers: _jsonHeaders,
        body: body,
        anySuccess: true);
  }

  Future<void> sendChatTyping({required String threadId}) async {
    final uri = _internal('/_fedi3/chat/typing');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'thread_id': threadId}),
      anySuccess: true,
    );
  }

  Future<void> editChatMessage(
      {required String messageId, required String text}) async {
    final uri = _internal('/_fedi3/chat/edit');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'message_id': messageId, 'text': text}),
      anySuccess: true,
    );
  }

  Future<void> sendChatReaction({
    required String messageId,
    required String reaction,
    bool remove = false,
  }) async {
    final uri = _internal('/_fedi3/chat/react');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({
        'message_id': messageId,
        'reaction': reaction,
        if (remove) 'remove': true,
      }),
      anySuccess: true,
    );
  }

  Future<Map<String, dynamic>> fetchChatReactions(
      {required List<String> messageIds}) async {
    final uri = _internal('/_fedi3/chat/reactions');
    final resp = await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'message_ids': messageIds}),
    );
    return _decodeMap(resp, 'chat reactions');
  }

  Future<void> deleteChatMessage({required String messageId}) async {
    final uri = _internal('/_fedi3/chat/delete');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'message_id': messageId}),
      anySuccess: true,
    );
  }

  Future<void> markChatSeen(
      {required String threadId, required String messageId}) async {
    final uri = _internal('/_fedi3/chat/seen');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'thread_id': threadId, 'message_id': messageId}),
      anySuccess: true,
    );
  }

  Future<Map<String, dynamic>> fetchChatStatuses(
      {required List<String> messageIds}) async {
    final uri = _internal('/_fedi3/chat/status');
    final resp = await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'message_ids': messageIds}),
    );
    return _decodeMap(resp, 'chat status');
  }

  Future<Map<String, dynamic>> fetchPresence() async {
    final uri = _internal('/_fedi3/presence');
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'presence');
  }

  Future<Map<String, dynamic>> setPresence(String status) async {
    final uri = _internal('/_fedi3/presence');
    final resp = await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'status': status}),
      anySuccess: true,
    );
    return _decodeMap(resp, 'presence');
  }

  // ---------------------------------------------------------------------------
  // Relays.
  // ---------------------------------------------------------------------------

  Future<Map<String, dynamic>> fetchRelays({int limit = 200}) async {
    final uri = _internal('/_fedi3/relays', {'limit': limit.toString()});
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    return _decodeMap(resp, 'relays');
  }

  Future<void> refreshRelays() async {
    final uri = _internal('/_fedi3/relays/refresh');
    await _request(
        method: 'POST', uri: uri, headers: _internalHeaders, anySuccess: true);
  }

  Future<Map<String, dynamic>> fetchRelayList() async {
    final uri = _relay('/_fedi3/relay/relays');
    final resp =
        await _request(method: 'GET', uri: uri, headers: _bearerHeaders);
    return _decodeMap(resp, 'relay list');
  }

  Future<Map<String, dynamic>> fetchRelayStats() async {
    final uri = _relay('/_fedi3/relay/stats');
    final resp =
        await _request(method: 'GET', uri: uri, headers: _bearerHeaders);
    return _decodeMap(resp, 'relay stats');
  }

  Future<Map<String, dynamic>> fetchRelayCoverage() async {
    final uri = _relay(
        '/_fedi3/relay/search/coverage', {'username': config.username.trim()});
    final resp =
        await _request(method: 'GET', uri: uri, headers: _bearerHeaders);
    return _decodeMap(resp, 'relay coverage');
  }

  Future<Map<String, dynamic>> fetchRelayPeers(
      {String? query, int limit = 200}) async {
    final params = <String, String>{'limit': limit.toString()};
    if (query != null && query.trim().isNotEmpty) {
      params['q'] = query.trim();
    }
    final uri = _relay('/_fedi3/relay/peers', params);
    final resp =
        await _request(method: 'GET', uri: uri, headers: _bearerHeaders);
    return _decodeMap(resp, 'relay peers');
  }

  Future<Map<String, dynamic>> fetchRelayMe() async {
    final uri =
        _relay('/_fedi3/relay/me', {'username': config.username.trim()});
    final resp =
        await _request(method: 'GET', uri: uri, headers: _bearerHeaders);
    return _decodeMap(resp, 'relay me');
  }

  // ---------------------------------------------------------------------------
  // Social actions (outbox).
  // ---------------------------------------------------------------------------

  Future<void> _postOutbox(Map<String, dynamic> activity) async {
    final user = Uri.encodeComponent(config.username.trim());
    final uri = _localOrRelay('/users/$user/outbox');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _activityHeaders,
      body: jsonEncode(activity),
      okStatus: const {202},
      anySuccess: true,
    );
  }

  Future<void> undoReaction({
    required String innerType,
    required String objectId,
    required String objectActor,
    String? innerId,
    String? content,
  }) async {
    final toActor = objectActor.trim();
    if (toActor.isEmpty) {
      throw CoreApiException(
          CoreApiErrorKind.badRequest, 'missing object actor');
    }
    final innerTypeNorm = innerType.trim();
    if (innerTypeNorm.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'missing innerType');
    }

    final inner = <String, dynamic>{
      'type': innerTypeNorm,
      'actor': _me,
      'object': objectId,
      if (innerId != null && innerId.trim().isNotEmpty) 'id': innerId.trim(),
      if (content != null && content.trim().isNotEmpty)
        'content': content.trim(),
    };

    await _postOutbox({
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Undo',
      'actor': _me,
      'object': inner,
      'to': [toActor],
    });
  }

  Future<void> react({
    required String objectId,
    required String objectActor,
    required String emoji,
  }) async {
    final toActor = objectActor.trim();
    if (toActor.isEmpty) {
      throw CoreApiException(
          CoreApiErrorKind.badRequest, 'missing object actor');
    }
    final e = emoji.trim();
    if (e.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'missing emoji');
    }

    await _postOutbox({
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'EmojiReact',
      'actor': _me,
      'to': [toActor],
      'object': objectId,
      'content': e,
    });
  }

  Future<void> follow(String actorUrl) async {
    final actor =
        (await _resolveActorInput(actorUrl)).replaceAll(RegExp(r'/+$'), '');
    final uri = _internal('/_fedi3/social/follow');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'actor': actor}),
      anySuccess: true,
    );
  }

  Future<void> unfollow(String actorUrl) async {
    final actor =
        (await _resolveActorInput(actorUrl)).replaceAll(RegExp(r'/+$'), '');
    final uri = _internal('/_fedi3/social/unfollow');
    await _request(
      method: 'POST',
      uri: uri,
      headers: _jsonHeaders,
      body: jsonEncode({'actor': actor}),
      anySuccess: true,
    );
  }

  Future<String> fetchFollowingStatus(String actorUrl) async {
    final actor =
        (await _resolveActorInput(actorUrl)).replaceAll(RegExp(r'/+$'), '');
    final uri = _internal('/_fedi3/social/status', {'actor': actor});
    final resp =
        await _request(method: 'GET', uri: uri, headers: _internalHeaders);
    try {
      final json = jsonDecode(resp.body);
      if (json is! Map) return 'none';
      final s = (json['status'] as String?)?.trim().toLowerCase() ?? '';
      if (s == 'pending' || s == 'accepted') return s;
      if (s == 'requested') return 'pending';
      if (s == 'following' || s == 'approved') return 'accepted';
      return 'none';
    } on FormatException catch (e) {
      throw CoreApiException(
          CoreApiErrorKind.decode, 'follow status returned invalid JSON',
          cause: e);
    }
  }

  // ---------------------------------------------------------------------------
  // H-44: pending follow requests for locked accounts.
  // ---------------------------------------------------------------------------

  /// GET /_fedi3/follow_requests. Returns the pending requests only.
  Future<List<FollowRequest>> fetchFollowRequests() async {
    final resp = await _request(
      method: 'GET',
      uri: _internal('/_fedi3/follow_requests'),
      headers: _internalHeaders,
    );
    try {
      final json = jsonDecode(resp.body);
      if (json is! List) {
        throw const FormatException('expected a JSON array');
      }
      return json
          .whereType<Map>()
          .map((e) => FollowRequest.fromJson(e.cast<String, dynamic>()))
          .where((r) => r.actor.isNotEmpty)
          .toList(growable: false);
    } on FormatException catch (e) {
      throw CoreApiException(
          CoreApiErrorKind.decode, 'follow requests returned invalid JSON',
          cause: e);
    }
  }

  /// POST /_fedi3/follow_requests/<actor>/approve.
  Future<void> approveFollowRequest(String actor) async {
    final trimmed = actor.trim();
    if (trimmed.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'missing actor');
    }
    final encoded = Uri.encodeComponent(trimmed);
    await _request(
      method: 'POST',
      uri: _internal('/_fedi3/follow_requests/$encoded/approve'),
      headers: _internalHeaders,
      anySuccess: true,
    );
  }

  /// POST /_fedi3/follow_requests/<actor>/reject.
  Future<void> rejectFollowRequest(String actor) async {
    final trimmed = actor.trim();
    if (trimmed.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'missing actor');
    }
    final encoded = Uri.encodeComponent(trimmed);
    await _request(
      method: 'POST',
      uri: _internal('/_fedi3/follow_requests/$encoded/reject'),
      headers: _internalHeaders,
      anySuccess: true,
    );
  }

  /// GET /_fedi3/identity/export. Returns the actor RSA key pair and the chat
  /// identity key pair used to build the multi-device identity package.
  Future<Map<String, dynamic>> exportIdentity() async {
    final resp = await _request(
      method: 'GET',
      uri: _internal('/_fedi3/identity/export'),
      headers: _internalHeaders,
    );
    return _decodeMap(resp, 'identity export');
  }

  /// GET /_fedi3/chat/snapshot. Returns the chat history bundle used to seed a
  /// newly linked client during QR pairing.
  Future<Map<String, dynamic>> exportChatSnapshot() async {
    final resp = await _request(
      method: 'GET',
      uri: _internal('/_fedi3/chat/snapshot'),
      headers: _internalHeaders,
    );
    return _decodeMap(resp, 'chat snapshot export');
  }

  /// POST /_fedi3/chat/snapshot. Restores a snapshot exported by another
  /// client of the same account.
  Future<void> importChatSnapshot(Map<String, dynamic> snapshot) async {
    await _request(
      method: 'POST',
      uri: _internal('/_fedi3/chat/snapshot'),
      headers: _jsonHeaders,
      body: snapshot,
      okStatus: const {200},
    );
  }

  Future<Map<String, dynamic>> uploadMedia({
    required Uint8List bytes,
    required String filename,
  }) async {
    final user = Uri.encodeComponent(config.username.trim());
    final uri = _localOrRelay('/users/$user/media');
    final mime = lookupMimeType(filename) ?? 'application/octet-stream';
    final resp = await _request(
      method: 'POST',
      uri: uri,
      headers: {
        ..._internalHeaders,
        'X-Filename': encodeHeaderValue(filename),
        'Content-Type': mime,
      },
      body: bytes,
      anySuccess: true,
    );
    return _decodeMap(resp, 'media upload');
  }

  Future<void> postNote({
    required String content,
    required bool public,
    required List<String> mediaIds,
    String? summary,
    bool sensitive = false,
    String visibility = 'public',
    String? directTo,
    String? inReplyTo,
    String? replyToActor,
  }) async {
    final followers = '$_me/followers';

    final mode = visibility.trim().isEmpty ? 'public' : visibility.trim();
    final to = <String>[];
    final cc = <String>[];
    final replyActor = replyToActor?.trim();
    if (mode == 'direct') {
      final targetInput = directTo?.trim() ?? '';
      if (targetInput.isEmpty) {
        throw CoreApiException(
            CoreApiErrorKind.badRequest, 'direct target missing');
      }
      final resolved = await _resolveActorInput(targetInput);
      to.add(resolved);
      if (replyActor != null &&
          replyActor.isNotEmpty &&
          replyActor != resolved) {
        to.add(replyActor);
      }
    } else if (mode == 'home' || mode == 'followers') {
      if (replyActor != null && replyActor.isNotEmpty) {
        to.add(replyActor);
      }
      if (to.isEmpty) {
        to.add(followers);
      }
    } else {
      to.add('https://www.w3.org/ns/activitystreams#Public');
      if (replyActor != null && replyActor.isNotEmpty) {
        to.add(replyActor);
      }
      cc.add(followers);
    }

    await _postOutbox({
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Create',
      'actor': _me,
      'to': to,
      if (cc.isNotEmpty) 'cc': cc,
      'object': {
        'type': 'Note',
        'content': content,
        if (summary != null && summary.trim().isNotEmpty)
          'summary': summary.trim(),
        if (sensitive) 'sensitive': true,
        if (inReplyTo != null && inReplyTo.trim().isNotEmpty)
          'inReplyTo': inReplyTo.trim(),
        if (mediaIds.isNotEmpty) 'attachment': mediaIds,
      },
    });
  }

  Future<void> editNote({
    required String objectId,
    required String content,
    required List<String> to,
    List<String>? cc,
    String? summary,
    bool sensitive = false,
    String? inReplyTo,
    List<dynamic>? attachments,
  }) async {
    final toList = to.where((v) => v.trim().isNotEmpty).toList();
    final ccList = (cc ?? const []).where((v) => v.trim().isNotEmpty).toList();
    if (toList.isEmpty && ccList.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'missing recipients');
    }

    final object = <String, dynamic>{
      'type': 'Note',
      'id': objectId,
      'content': content,
    };
    if (summary != null && summary.trim().isNotEmpty) {
      object['summary'] = summary.trim();
    }
    if (sensitive) {
      object['sensitive'] = true;
    }
    if (inReplyTo != null && inReplyTo.trim().isNotEmpty) {
      object['inReplyTo'] = inReplyTo.trim();
    }
    if (attachments != null && attachments.isNotEmpty) {
      object['attachment'] = attachments;
    }

    await _postOutbox({
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Update',
      'actor': _me,
      'to': toList,
      if (ccList.isNotEmpty) 'cc': ccList,
      'object': object,
    });
  }

  Future<void> deleteNote({
    required String objectId,
    required List<String> to,
    List<String>? cc,
    String objectType = 'Note',
  }) async {
    final toList = to.where((v) => v.trim().isNotEmpty).toList();
    final ccList = (cc ?? const []).where((v) => v.trim().isNotEmpty).toList();
    if (toList.isEmpty && ccList.isEmpty) {
      throw CoreApiException(CoreApiErrorKind.badRequest, 'missing recipients');
    }

    final tombstone = {
      'id': objectId,
      'type': 'Tombstone',
      'formerType': objectType,
      'deleted': DateTime.now().toUtc().toIso8601String(),
    };

    await _postOutbox({
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Delete',
      'actor': _me,
      'to': toList,
      if (ccList.isNotEmpty) 'cc': ccList,
      'object': tombstone,
    });
  }

  Future<void> boost({
    required String objectId,
    bool public = true,
  }) async {
    final followers = '$_me/followers';
    final to =
        public ? ['https://www.w3.org/ns/activitystreams#Public'] : [followers];
    final cc = public ? [followers] : null;

    await _postOutbox({
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Announce',
      'actor': _me,
      'to': to,
      if (cc != null) 'cc': cc,
      'object': objectId,
    });
  }

  Future<void> like({
    required String objectId,
    required String objectActor,
  }) async {
    final toActor = objectActor.trim();
    if (toActor.isEmpty) {
      throw CoreApiException(
          CoreApiErrorKind.badRequest, 'missing object actor');
    }

    await _postOutbox({
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Like',
      'actor': _me,
      'to': [toActor],
      'object': objectId,
    });
  }
}
