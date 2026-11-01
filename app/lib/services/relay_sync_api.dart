/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:http/http.dart' as http;

import '../model/core_config.dart';
import '../model/relay_sync_models.dart';

class RelaySyncApi {
  RelaySyncApi({
    required this.config,
    this.fallbackTokens = const <String>[],
  });

  final CoreConfig config;
  final List<String> fallbackTokens;

  Uri _public(String path, [Map<String, String>? query]) {
    final base = Uri.parse(config.publicBaseUrl.trim());
    return base.replace(path: path, queryParameters: query);
  }

  List<String> get _authTokens {
    final values = <String>[
      config.relayToken.trim(),
      for (final t in fallbackTokens) t.trim(),
    ];
    final seen = <String>{};
    final out = <String>[];
    for (final t in values) {
      if (t.isEmpty || !seen.add(t)) continue;
      out.add(t);
    }
    return out;
  }

  Map<String, String> _headersForToken(String token, {bool jsonBody = false}) => {
        'Authorization': 'Bearer $token',
        'Accept': 'application/json',
        if (jsonBody) 'Content-Type': 'application/json',
      };

  Map<String, String> _query(Map<String, String?> raw) {
    return {
      'username': config.username.trim(),
      for (final entry in raw.entries)
        if (entry.value != null && entry.value!.trim().isNotEmpty)
          entry.key: entry.value!.trim(),
    };
  }

  Future<RelaySyncBootstrapSnapshot> fetchBootstrap({
    int eventLimit = 100,
    int notificationLimit = 100,
    int chatLimit = 100,
    int timelineLimit = 100,
  }) async {
    final resp = await _getWithAuthRetry(
      _public(
        '/sync/bootstrap',
        _query({
          'event_limit': '$eventLimit',
          'notification_limit': '$notificationLimit',
          'chat_limit': '$chatLimit',
          'timeline_limit': '$timelineLimit',
        }),
      ),
    );
    return RelaySyncBootstrapSnapshot.fromJson(_decode(resp, 'sync bootstrap'));
  }

  Future<Map<String, dynamic>> fetchEvents({
    int limit = 200,
    int? sinceId,
    int? cursorId,
  }) async {
    final resp = await _getWithAuthRetry(
      _public(
        '/sync/events',
        _query({
          'limit': '$limit',
          if (sinceId != null && sinceId > 0) 'since_id': '$sinceId',
          if (cursorId != null && cursorId > 0) 'cursor_id': '$cursorId',
        }),
      ),
    );
    return _decode(resp, 'sync events');
  }

  Future<Map<String, dynamic>> fetchNotifications({
    int limit = 200,
    int? sinceId,
    int? cursorId,
  }) async {
    final resp = await _getWithAuthRetry(
      _public(
        '/sync/notifications',
        _query({
          'limit': '$limit',
          if (sinceId != null && sinceId > 0) 'since_id': '$sinceId',
          if (cursorId != null && cursorId > 0) 'cursor_id': '$cursorId',
        }),
      ),
    );
    return _decode(resp, 'sync notifications');
  }

  Future<Map<String, dynamic>> fetchChat({
    int limit = 200,
    int? sinceId,
    int? cursorId,
  }) async {
    final resp = await _getWithAuthRetry(
      _public(
        '/sync/chat',
        _query({
          'limit': '$limit',
          if (sinceId != null && sinceId > 0) 'since_id': '$sinceId',
          if (cursorId != null && cursorId > 0) 'cursor_id': '$cursorId',
        }),
      ),
    );
    return _decode(resp, 'sync chat');
  }

  Future<Map<String, dynamic>> fetchTimelineHome({
    int limit = 200,
    int? since,
    int? cursor,
  }) async {
    final resp = await _getWithAuthRetry(
      _public(
        '/sync/timeline/home',
        _query({
          'limit': '$limit',
          if (since != null && since > 0) 'since': '$since',
          if (cursor != null && cursor > 0) 'cursor': '$cursor',
        }),
      ),
    );
    return _decode(resp, 'sync timeline home');
  }

  Future<Map<String, dynamic>> postChatEnvelope({
    required String threadId,
    required String messageId,
    required String senderActor,
    required List<String> recipientUsers,
    required Map<String, dynamic> envelope,
    String? senderUser,
    int? createdAtMs,
  }) async {
    final resp = await _postWithAuthRetry(
      _public('/sync/chat/envelope'),
      body: jsonEncode({
        'username': config.username.trim(),
        'thread_id': threadId,
        'message_id': messageId,
        'sender_actor': senderActor,
        'sender_user': senderUser,
        'recipient_users': recipientUsers,
        'envelope': envelope,
        if (createdAtMs != null) 'created_at_ms': createdAtMs,
      }),
    );
    return _decode(resp, 'chat envelope post');
  }

  Future<void> ackChatMessage({
    required String deviceId,
    required String messageId,
    int? ackedAtMs,
  }) async {
    final resp = await _postWithAuthRetry(
      _public('/sync/chat/ack'),
      body: jsonEncode({
        'username': config.username.trim(),
        'device_id': deviceId,
        'message_id': messageId,
        if (ackedAtMs != null) 'acked_at_ms': ackedAtMs,
      }),
    );
    _decode(resp, 'chat ack');
  }

  Future<Map<String, dynamic>> deleteChatMessage({
    required String threadId,
    required String messageId,
    int? deletedAtMs,
  }) async {
    final resp = await _postWithAuthRetry(
      _public('/sync/chat/delete'),
      body: jsonEncode({
        'username': config.username.trim(),
        'thread_id': threadId,
        'message_id': messageId,
        if (deletedAtMs != null) 'deleted_at_ms': deletedAtMs,
      }),
    );
    return _decode(resp, 'chat delete');
  }

  Future<Map<String, dynamic>> deleteChatThread({
    required String threadId,
    int? deletedAtMs,
  }) async {
    final resp = await _postWithAuthRetry(
      _public('/sync/chat/thread/delete'),
      body: jsonEncode({
        'username': config.username.trim(),
        'thread_id': threadId,
        if (deletedAtMs != null) 'deleted_at_ms': deletedAtMs,
      }),
    );
    return _decode(resp, 'chat thread delete');
  }

  Future<http.Response> _getWithAuthRetry(Uri uri) async {
    final tokens = _authTokens;
    if (tokens.isEmpty) {
      return http.get(uri, headers: const {'Accept': 'application/json'});
    }
    http.Response? last;
    for (final token in tokens) {
      final resp = await http.get(uri, headers: _headersForToken(token));
      last = resp;
      if (resp.statusCode != 401) return resp;
    }
    return last!;
  }

  Future<http.Response> _postWithAuthRetry(Uri uri, {required String body}) async {
    final tokens = _authTokens;
    if (tokens.isEmpty) {
      return http.post(
        uri,
        headers: const {
          'Accept': 'application/json',
          'Content-Type': 'application/json',
        },
        body: body,
      );
    }
    http.Response? last;
    for (final token in tokens) {
      final resp = await http.post(
        uri,
        headers: _headersForToken(token, jsonBody: true),
        body: body,
      );
      last = resp;
      if (resp.statusCode != 401) return resp;
    }
    return last!;
  }

  Map<String, dynamic> _decode(http.Response resp, String label) {
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('$label failed: ${resp.statusCode} ${resp.body}');
    }
    final decoded = jsonDecode(resp.body);
    if (decoded is! Map) {
      throw StateError('$label returned invalid json');
    }
    return decoded.cast<String, dynamic>();
  }
}
