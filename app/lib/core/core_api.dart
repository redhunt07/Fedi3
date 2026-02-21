/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:mime/mime.dart';
import '../model/core_config.dart';

class CoreApi {
  CoreApi({required this.config});

  final CoreConfig config;

  Map<String, String> get _internalHeaders => {
        if (config.internalToken.trim().isNotEmpty) 'X-Fedi3-Internal': config.internalToken.trim(),
      };

  bool get _bindLooksLocal {
    final host = Uri.parse('http://${config.bind}').host.toLowerCase();
    return host == '127.0.0.1' || host == 'localhost' || host == '0.0.0.0' || host == '::1';
  }

  Uri _baseLocalOrRelay() {
    if (_bindLooksLocal) return config.localBaseUri;
    return Uri.parse(config.publicBaseUrl.trim());
  }

  Uri _internalBase(String path) {
    return config.localBaseUri.replace(path: path);
  }

  Uri _localOrRelay(String path, [Map<String, String>? query]) {
    return _baseLocalOrRelay().replace(
      path: path,
      queryParameters: query,
    );
  }

  Uri _internal(String path, [Map<String, String>? query]) {
    return _internalBase(path).replace(
      queryParameters: query,
    );
  }

  Future<String> _resolveActorInput(String input) async {
    var v = input.trim();
    if (v.isEmpty) throw StateError('missing actor');

    var user = '';
    var host = '';

    if (v.startsWith('http://') || v.startsWith('https://')) {
      final uri = Uri.tryParse(v);
      if (uri == null || uri.host.isEmpty) throw StateError('invalid url');
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
      if (parts.length < 2) throw StateError('invalid handle');
      user = parts.first;
      host = parts.last;
    } else {
      throw StateError('unsupported actor format');
    }

    if (user.isEmpty || host.isEmpty) {
      throw StateError('invalid actor');
    }

    final wf = Uri.https(host, '/.well-known/webfinger', {'resource': 'acct:$user@$host'});

    final client = _createHttpClient();
    try {
      final resp = await client.get(wf, headers: {'Accept': 'application/jrd+json, application/json'});
      if (resp.statusCode < 200 || resp.statusCode >= 300) {
        throw StateError('webfinger failed: ${resp.statusCode} ${resp.body}');
      }
      final json = jsonDecode(resp.body);
      if (json is! Map) throw StateError('webfinger invalid json');
      final links = (json['links'] as List<dynamic>? ?? []);
      for (final link in links) {
        if (link is! Map) continue;
        final rel = link['rel']?.toString();
        final href = link['href']?.toString();
        final type = link['type']?.toString() ?? '';
        if (rel == 'self' && href != null && href.isNotEmpty) {
          if (type.contains('activity+json') || type.contains('application/json') || type.isEmpty) {
            return href.replaceAll(RegExp(r'/+$'), '');
          }
        }
      }
      for (final link in links) {
        if (link is! Map) continue;
        final href = link['href']?.toString();
        if (href != null && href.isNotEmpty) return href.replaceAll(RegExp(r'/+$'), '');
      }
      throw StateError('webfinger: no actor link');
    } finally {
      client.close();
    }
  }

  Future<String> resolveActorInput(String input) async {
    return _resolveActorInput(input);
  }

  Future<Map<String, dynamic>> fetchTimeline(String kind, {String? cursor, int limit = 50}) async {
    final uri = _internal('/_fedi3/timeline/$kind', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('timeline $kind failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> triggerLegacySync({int pages = 6, int itemsPerActor = 200}) async {
    final uri = _internal('/_fedi3/sync/legacy', {
      'pages': pages.toString(),
      'items': itemsPerActor.toString(),
      'include_fedi3': '1',
    });
    final resp = await http.post(uri, headers: {..._internalHeaders, 'Content-Type': 'application/json'}, body: '{}');
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('legacy sync failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> exportBackup() async {
    final uri = _internal('/_fedi3/backup/export');
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('backup export failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> importBackup(Map<String, dynamic> payload) async {
    final uri = _internal('/_fedi3/backup/import');
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: jsonEncode(payload),
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('backup import failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>?> fetchCachedObject(String url) async {
    final u = url.trim();
    if (u.isEmpty) return null;
    final uri = _internal('/_fedi3/object', {'url': u});
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode == 404) return null;
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('object fetch failed: ${resp.statusCode} ${resp.body}');
    }
    final json = jsonDecode(resp.body);
    if (json is! Map) return null;
    return json.cast<String, dynamic>();
  }

  Future<Map<String, dynamic>> fetchNoteReplies(String noteId, {String? cursor, int limit = 20}) async {
    final uri = _internal('/_fedi3/note/replies', {
      'note': noteId,
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('note replies failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> setNotePinned({required String noteId, required bool pinned}) async {
    final uri = _internal('/_fedi3/note/pin');
    final payload = {'id': noteId, 'pinned': pinned};
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: jsonEncode(payload),
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('note pin failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<List<Map<String, dynamic>>> fetchPinnedNotes({int limit = 20}) async {
    final uri = _internal('/_fedi3/note/pinned', {'limit': limit.toString()});
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('pinned notes failed: ${resp.statusCode} ${resp.body}');
    }
    final json = jsonDecode(resp.body);
    if (json is! Map) return const [];
    final items = json['items'];
    if (items is! List) return const [];
    return items.whereType<Map>().map((v) => v.cast<String, dynamic>()).toList();
  }

  Future<Map<String, dynamic>> fetchReactions(String objectId, {int limit = 50}) async {
    final uri = _internal('/_fedi3/reactions', {
      'object': objectId,
      'limit': limit.toString(),
    });
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('reactions failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchMyReactions(String objectId) async {
    final uri = _internal('/_fedi3/reactions/me', {'object': objectId});
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('my reactions failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
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
      if (consistency != null && consistency.trim().isNotEmpty) 'consistency': consistency.trim(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    };
    final uri = _internal('/_fedi3/search/notes', params);
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('search notes failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
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
      if (consistency != null && consistency.trim().isNotEmpty) 'consistency': consistency.trim(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    };
    final uri = _internal('/_fedi3/search/users', params);
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('search users failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchChatThreads({
    String? cursor,
    int limit = 50,
    bool archived = false,
  }) async {
    final uri = _internal('/_fedi3/chat/threads', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
      if (archived) 'archived': '1',
    });
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat threads failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchChatMembers({required String threadId}) async {
    final uri = _internal('/_fedi3/chat/thread/members', {'thread_id': threadId});
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode == 404) {
      return {'items': []};
    }
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat members failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchChatMessages({
    required String threadId,
    String? cursor,
    int limit = 50,
  }) async {
    final uri = _internal('/_fedi3/chat/threads/$threadId', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat messages failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
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
      if (threadId != null && threadId.trim().isNotEmpty) 'thread_id': threadId.trim(),
      'recipients': recipients,
      'text': text,
      if (replyTo != null && replyTo.trim().isNotEmpty) 'reply_to': replyTo.trim(),
      if (title != null && title.trim().isNotEmpty) 'title': title.trim(),
      if (attachments != null && attachments.isNotEmpty) 'attachments': attachments,
    });
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat send failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> updateChatThreadTitle({required String threadId, required String title}) async {
    final uri = _internal('/_fedi3/chat/thread/update');
    final body = jsonEncode({'thread_id': threadId, 'title': title});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat thread update failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> deleteChatThread({required String threadId}) async {
    debugPrint('[DEBUG] CoreApi.deleteChatThread: threadId = $threadId');
    final uri = _internal('/_fedi3/chat/thread/delete');
    debugPrint('[DEBUG] CoreApi.deleteChatThread: URI = $uri');
    final body = jsonEncode({'thread_id': threadId});
    debugPrint('[DEBUG] CoreApi.deleteChatThread: body = $body');
    debugPrint('[DEBUG] CoreApi.deleteChatThread: headers = $_internalHeaders');
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    debugPrint('[DEBUG] CoreApi.deleteChatThread: response status = ${resp.statusCode}');
    debugPrint('[DEBUG] CoreApi.deleteChatThread: response body = ${resp.body}');
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      debugPrint('[DEBUG] CoreApi.deleteChatThread: ERRORE - sollevando eccezione');
      throw StateError('chat thread delete failed: ${resp.statusCode} ${resp.body}');
    }
    debugPrint('[DEBUG] CoreApi.deleteChatThread: successo');
  }

  Future<void> leaveChatThread({required String threadId}) async {
    final uri = _internal('/_fedi3/chat/thread/leave');
    final body = jsonEncode({'thread_id': threadId});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat thread leave failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> archiveChatThread({required String threadId, bool archived = true}) async {
    final uri = _internal('/_fedi3/chat/thread/archive');
    final body = jsonEncode({'thread_id': threadId, 'archived': archived});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat thread archive failed: ${resp.statusCode} ${resp.body}');
    }
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
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat thread members failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> sendChatTyping({required String threadId}) async {
    final uri = _internal('/_fedi3/chat/typing');
    final body = jsonEncode({'thread_id': threadId});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat typing failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> editChatMessage({required String messageId, required String text}) async {
    final uri = _internal('/_fedi3/chat/edit');
    final body = jsonEncode({'message_id': messageId, 'text': text});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat edit failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> fetchRelays({int limit = 200}) async {
    final uri = _internal('/_fedi3/relays', {'limit': limit.toString()});
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relays failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> refreshRelays() async {
    final uri = _internal('/_fedi3/relays/refresh');
    final resp = await http.post(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relays refresh failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> sendChatReaction({
    required String messageId,
    required String reaction,
    bool remove = false,
  }) async {
    final uri = _internal('/_fedi3/chat/react');
    final body = jsonEncode({
      'message_id': messageId,
      'reaction': reaction,
      if (remove) 'remove': true,
    });
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat react failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> fetchChatReactions({required List<String> messageIds}) async {
    final uri = _internal('/_fedi3/chat/reactions');
    final body = jsonEncode({'message_ids': messageIds});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat reactions failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> deleteChatMessage({required String messageId}) async {
    final uri = _internal('/_fedi3/chat/delete');
    final body = jsonEncode({'message_id': messageId});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat delete failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> markChatSeen({required String threadId, required String messageId}) async {
    final uri = _internal('/_fedi3/chat/seen');
    final body = jsonEncode({'thread_id': threadId, 'message_id': messageId});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat seen failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> fetchChatStatuses({required List<String> messageIds}) async {
    final uri = _internal('/_fedi3/chat/status');
    final body = jsonEncode({'message_ids': messageIds});
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('chat status failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
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
      if (consistency != null && consistency.trim().isNotEmpty) 'consistency': consistency.trim(),
    };
    final uri = _internal('/_fedi3/search/hashtags', params);
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('search hashtags failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
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
      if (content != null && content.trim().isNotEmpty) 'content': content.trim(),
      'limit': limit.toString(),
    });
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('reaction actors failed: ${resp.statusCode} ${resp.body}');
    }
    final json = jsonDecode(resp.body);
    if (json is! Map) return const [];
    final items = json['items'];
    if (items is! List) return const [];
    return items.whereType<String>().map((s) => s.trim()).where((s) => s.isNotEmpty).toList();
  }

  Future<void> undoReaction({
    required String innerType,
    required String objectId,
    required String objectActor,
    String? innerId,
    String? content,
  }) async {
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = config.username.trim();
    final me = '$base/users/$user';
    final toActor = objectActor.trim();
    if (toActor.isEmpty) throw StateError('missing object actor');
    final innerTypeNorm = innerType.trim();
    if (innerTypeNorm.isEmpty) throw StateError('missing innerType');

    final inner = <String, dynamic>{
      'type': innerTypeNorm,
      'actor': me,
      'object': objectId,
      if (innerId != null && innerId.trim().isNotEmpty) 'id': innerId.trim(),
      if (content != null && content.trim().isNotEmpty) 'content': content.trim(),
    };

    final activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Undo',
      'actor': me,
      'object': inner,
      'to': [toActor],
    };

    final uri = _localOrRelay('/users/$user/outbox');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      },
      body: jsonEncode(activity),
    );
    if (resp.statusCode != 202 && (resp.statusCode < 200 || resp.statusCode >= 300)) {
      throw StateError('undo failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> fetchNotifications({String? cursor, int limit = 50}) async {
    final uri = _internal('/_fedi3/notifications', {
      'limit': limit.toString(),
      if (cursor != null && cursor.trim().isNotEmpty) 'cursor': cursor.trim(),
    });
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('notifications failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchMigrationStatus() async {
    final uri = _internal('/_fedi3/migration/status');
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('migration status failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> setLegacyAliases(List<String> aliases) async {
    final uri = _internal('/_fedi3/migration/legacy_aliases');
    final payload = {
      'aliases': aliases,
    };
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: jsonEncode(payload),
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('migration aliases failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> react({
    required String objectId,
    required String objectActor,
    required String emoji,
  }) async {
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = config.username.trim();
    final me = '$base/users/$user';
    final toActor = objectActor.trim();
    if (toActor.isEmpty) throw StateError('missing object actor');
    final e = emoji.trim();
    if (e.isEmpty) throw StateError('missing emoji');

    final activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'EmojiReact',
      'actor': me,
      'to': [toActor],
      'object': objectId,
      'content': e,
    };

    final uri = _localOrRelay('/users/$user/outbox');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      },
      body: jsonEncode(activity),
    );
    if (resp.statusCode != 202 && (resp.statusCode < 200 || resp.statusCode >= 300)) {
      throw StateError('react failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> follow(String actorUrl) async {
    final uri = _internal('/_fedi3/social/follow');
    final actor = (await _resolveActorInput(actorUrl)).replaceAll(RegExp(r'/+$'), '');
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: jsonEncode({'actor': actor}),
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('follow failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> unfollow(String actorUrl) async {
    final uri = _internal('/_fedi3/social/unfollow');
    final actor = (await _resolveActorInput(actorUrl)).replaceAll(RegExp(r'/+$'), '');
    final resp = await http.post(
      uri,
      headers: {..._internalHeaders, 'Content-Type': 'application/json'},
      body: jsonEncode({'actor': actor}),
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('unfollow failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<String> fetchFollowingStatus(String actorUrl) async {
    final actor = (await _resolveActorInput(actorUrl)).replaceAll(RegExp(r'/+$'), '');
    final uri = _internal('/_fedi3/social/status', {'actor': actor});
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('follow status failed: ${resp.statusCode} ${resp.body}');
    }
    final json = jsonDecode(resp.body);
    if (json is! Map) return 'none';
    final s = (json['status'] as String?)?.trim().toLowerCase() ?? '';
    if (s == 'pending' || s == 'accepted') return s;
    if (s == 'requested') return 'pending';
    if (s == 'following' || s == 'approved') return 'accepted';
    return 'none';
  }

  Future<Map<String, dynamic>> uploadMedia({
    required Uint8List bytes,
    required String filename,
  }) async {
    final user = config.username.trim();
    final uri = _localOrRelay('/users/$user/media');
    final mime = lookupMimeType(filename) ?? 'application/octet-stream';
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'X-Filename': filename,
        'Content-Type': mime,
      },
      body: bytes,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('media upload failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
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
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = config.username.trim();
    final me = '$base/users/$user';
    final followers = '$me/followers';

    final mode = visibility.trim().isEmpty ? 'public' : visibility.trim();
    final to = <String>[];
    final cc = <String>[];
    final replyActor = replyToActor?.trim();
    if (mode == 'direct') {
      final targetInput = directTo?.trim() ?? '';
      if (targetInput.isEmpty) {
        throw StateError('direct target missing');
      }
      final resolved = await _resolveActorInput(targetInput);
      to.add(resolved);
      if (replyActor != null && replyActor.isNotEmpty && replyActor != resolved) {
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

    final activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Create',
      'actor': me,
      'to': to,
      if (cc.isNotEmpty) 'cc': cc,
      'object': {
        'type': 'Note',
        'content': content,
        if (summary != null && summary.trim().isNotEmpty) 'summary': summary.trim(),
        if (sensitive) 'sensitive': true,
        if (inReplyTo != null && inReplyTo.trim().isNotEmpty) 'inReplyTo': inReplyTo.trim(),
        if (mediaIds.isNotEmpty) 'attachment': mediaIds,
      },
    };

    final uri = _localOrRelay('/users/$user/outbox');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      },
      body: jsonEncode(activity),
    );
    if (resp.statusCode != 202 && (resp.statusCode < 200 || resp.statusCode >= 300)) {
      throw StateError('post failed: ${resp.statusCode} ${resp.body}');
    }
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
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = config.username.trim();
    final me = '$base/users/$user';

    final toList = to.where((v) => v.trim().isNotEmpty).toList();
    final ccList = (cc ?? const []).where((v) => v.trim().isNotEmpty).toList();
    if (toList.isEmpty && ccList.isEmpty) {
      throw StateError('missing recipients');
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

    final activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Update',
      'actor': me,
      'to': toList,
      if (ccList.isNotEmpty) 'cc': ccList,
      'object': object,
    };

    final uri = _localOrRelay('/users/$user/outbox');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      },
      body: jsonEncode(activity),
    );
    if (resp.statusCode != 202 && (resp.statusCode < 200 || resp.statusCode >= 300)) {
      throw StateError('edit failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> deleteNote({
    required String objectId,
    required List<String> to,
    List<String>? cc,
    String objectType = 'Note',
  }) async {
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = config.username.trim();
    final me = '$base/users/$user';

    final toList = to.where((v) => v.trim().isNotEmpty).toList();
    final ccList = (cc ?? const []).where((v) => v.trim().isNotEmpty).toList();
    if (toList.isEmpty && ccList.isEmpty) {
      throw StateError('missing recipients');
    }

    final tombstone = {
      'id': objectId,
      'type': 'Tombstone',
      'formerType': objectType,
      'deleted': DateTime.now().toUtc().toIso8601String(),
    };

    final activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Delete',
      'actor': me,
      'to': toList,
      if (ccList.isNotEmpty) 'cc': ccList,
      'object': tombstone,
    };

    final uri = _localOrRelay('/users/$user/outbox');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      },
      body: jsonEncode(activity),
    );
    if (resp.statusCode != 202 && (resp.statusCode < 200 || resp.statusCode >= 300)) {
      throw StateError('delete failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> boost({
    required String objectId,
    bool public = true,
  }) async {
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = config.username.trim();
    final me = '$base/users/$user';
    final followers = '$me/followers';
    final to = public ? ['https://www.w3.org/ns/activitystreams#Public'] : [followers];
    final cc = public ? [followers] : null;

    final activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Announce',
      'actor': me,
      'to': to,
      if (cc != null) 'cc': cc,
      'object': objectId,
    };

    final uri = _localOrRelay('/users/$user/outbox');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      },
      body: jsonEncode(activity),
    );
    if (resp.statusCode != 202 && (resp.statusCode < 200 || resp.statusCode >= 300)) {
      throw StateError('boost failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> like({
    required String objectId,
    required String objectActor,
  }) async {
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = config.username.trim();
    final me = '$base/users/$user';
    final toActor = objectActor.trim();
    if (toActor.isEmpty) throw StateError('missing object actor');

    final activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      'type': 'Like',
      'actor': me,
      'to': [toActor],
      'object': objectId,
    };

    final uri = _localOrRelay('/users/$user/outbox');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json',
      },
      body: jsonEncode(activity),
    );
    if (resp.statusCode != 202 && (resp.statusCode < 200 || resp.statusCode >= 300)) {
      throw StateError('like failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> fetchRelayList() async {
    final base = Uri.parse(config.publicBaseUrl.trim());
    final uri = base.replace(path: '/_fedi3/relay/relays');
    final resp = await http.get(uri);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relay list failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchRelayStats() async {
    final base = Uri.parse(config.publicBaseUrl.trim());
    final uri = base.replace(path: '/_fedi3/relay/stats');
    final resp = await http.get(uri);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relay stats failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchRelayCoverage() async {
    final base = Uri.parse(config.publicBaseUrl.trim());
    final uri = base.replace(
      path: '/_fedi3/relay/search/coverage',
      queryParameters: {'username': config.username.trim()},
    );
    final resp = await http.get(
      uri,
      headers: {
        'Authorization': 'Bearer ${config.relayToken.trim()}',
      },
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relay coverage failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> fetchRelayPeers({String? query, int limit = 200}) async {
    final base = Uri.parse(config.publicBaseUrl.trim());
    final params = <String, String>{'limit': limit.toString()};
    if (query != null && query.trim().isNotEmpty) {
      params['q'] = query.trim();
    }
    final uri = base.replace(path: '/_fedi3/relay/peers', queryParameters: params);
    final resp = await http.get(uri);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relay peers failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Stream<Map<String, dynamic>> relayPresenceStream() async* {
    final base = Uri.parse(config.publicBaseUrl.trim());
    final uri = base.replace(path: '/_fedi3/relay/presence/stream');
    final client = http.Client();
    try {
      final req = http.Request('GET', uri);
      final resp = await client.send(req);
      if (resp.statusCode < 200 || resp.statusCode >= 300) {
        final body = await resp.stream.bytesToString();
        throw StateError('relay presence stream failed: ${resp.statusCode} $body');
      }
      String? event;
      final data = StringBuffer();
      await for (final line in resp.stream.transform(utf8.decoder).transform(const LineSplitter())) {
        if (line.startsWith('event:')) {
          event = line.substring(6).trim();
          continue;
        }
        if (line.startsWith('data:')) {
          data.writeln(line.substring(5).trim());
          continue;
        }
        if (line.trim().isNotEmpty) continue;
        if (data.isEmpty) {
          event = null;
          continue;
        }
        final raw = data.toString().trim();
        data.clear();
        if (raw.isEmpty) {
          event = null;
          continue;
        }
        final decoded = jsonDecode(raw);
        if (decoded is Map<String, dynamic>) {
          decoded['event'] = event ?? 'message';
          yield decoded;
        } else {
          yield {'event': event ?? 'message', 'data': decoded};
        }
        event = null;
      }
    } finally {
      client.close();
    }
  }

  Future<Map<String, dynamic>> fetchRelayMe() async {
    final base = Uri.parse(config.publicBaseUrl.trim());
    final uri = base.replace(
      path: '/_fedi3/relay/me',
      queryParameters: {'username': config.username.trim()},
    );
    final resp = await http.get(
      uri,
      headers: {
        'Authorization': 'Bearer ${config.relayToken.trim()}',
      },
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relay me failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> sendClientTelemetry(Map<String, dynamic> payload) async {
    final base = Uri.parse(config.publicBaseUrl.trim());
    final uri = base.replace(path: '/_fedi3/relay/telemetry/client');
    final headers = <String, String>{
      'Content-Type': 'application/json',
    };
    if (config.relayToken.trim().isNotEmpty) {
      headers['Authorization'] = 'Bearer ${config.relayToken.trim()}';
    }
    final resp = await http.post(uri, headers: headers, body: jsonEncode(payload));
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('relay client telemetry failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> fetchNetMetrics() async {
    final uri = _internal('/_fedi3/net/metrics');
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('net metrics failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<void> refreshProfile() async {
    final uri = _internal('/_fedi3/profile/refresh');
    final resp = await http.post(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('profile refresh failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<Map<String, dynamic>> fetchUpnpStatus() async {
    final uri = _internal('/_fedi3/core/upnp');
    final resp = await http.get(uri, headers: _internalHeaders);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('upnp status failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> toggleUpnp({required bool enable}) async {
    final uri = _internal('/_fedi3/core/upnp');
    final resp = await http.post(
      uri,
      headers: {
        ..._internalHeaders,
        'Content-Type': 'application/json',
      },
      body: jsonEncode({
        'enabled': enable,
      }),
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('upnp toggle failed: ${resp.statusCode} ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  http.Client _createHttpClient() {
    final client = http.Client();
    
    // Standard anonymous User-Agent
    const userAgent = 'Fedi3/0.1 (+https://fedi3)';
    
    // Apply proxy settings if configured
    if (config.useTor || (config.proxyHost != null && config.proxyPort != null)) {
      final proxyConfig = ProxyConfig(
        host: config.proxyHost ?? '127.0.0.1',
        port: config.proxyPort ?? 9050,
        type: config.proxyType == 'socks5' ? ProxyType.socks5 : ProxyType.http,
      );
      
      return ProxyClient(
        proxy: proxyConfig,
        inner: client,
        defaultHeaders: {'User-Agent': userAgent},
      );
    }
    
    // Return regular client with anonymous User-Agent
    return UserAgentClient(userAgent, client);
  }
}

class UserAgentClient extends http.BaseClient {
  final String userAgent;
  final http.Client _inner;

  UserAgentClient(this.userAgent, this._inner);

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) {
    request.headers['User-Agent'] = userAgent;
    return _inner.send(request);
  }

  @override
  void close() {
    _inner.close();
  }
}

// Proxy configuration classes
enum ProxyType { http, socks5 }

class ProxyConfig {
  final String host;
  final int port;
  final ProxyType type;

  ProxyConfig({required this.host, required this.port, required this.type});
}

class ProxyClient extends http.BaseClient {
  final ProxyConfig proxy;
  final http.Client _inner;
  final Map<String, String> defaultHeaders;

  ProxyClient({
    required this.proxy,
    required http.Client inner,
    required this.defaultHeaders,
  }) : _inner = inner;

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) {
    // Apply default headers
    defaultHeaders.forEach((key, value) {
      if (!request.headers.containsKey(key)) {
        request.headers[key] = value;
      }
    });

    // For now, just return the inner client's response
    // In a real implementation, this would handle proxy logic
    return _inner.send(request);
  }

  @override
  void close() {
    _inner.close();
  }
}
