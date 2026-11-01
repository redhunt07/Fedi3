/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import '../model/core_config.dart';
import '../model/relay_sync_models.dart';

class RelaySyncStreamClient {
  RelaySyncStreamClient({
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

  Future<Stream<RelaySyncStreamEvent>> connect({int? sinceId}) async {
    final uri = _public('/sync/stream', {
      'username': config.username.trim(),
      if (sinceId != null && sinceId > 0) 'since_id': '$sinceId',
    });
    final tokens = _authTokens;
    if (tokens.isEmpty) {
      throw StateError('sync stream requires relay token');
    }
    final client = http.Client();
    http.StreamedResponse? last;
    for (final token in tokens) {
      final req = http.Request('GET', uri)
        ..headers.addAll({
          'Authorization': 'Bearer $token',
          'Accept': 'text/event-stream',
          'Cache-Control': 'no-cache',
          if (sinceId != null && sinceId > 0) 'Last-Event-ID': '$sinceId',
        });
      final resp = await client.send(req);
      last = resp;
      if (resp.statusCode != 401) {
        if (resp.statusCode < 200 || resp.statusCode >= 300) {
          final body = await resp.stream.bytesToString();
          client.close();
          throw StateError(
              'sync stream failed: ${resp.statusCode} ${body.trim()}');
        }
        return _parseStream(resp, client);
      }
    }
    final body = await last!.stream.bytesToString();
    client.close();
    throw StateError('sync stream failed: ${last.statusCode} ${body.trim()}');
  }

  Stream<RelaySyncStreamEvent> _parseStream(
    http.StreamedResponse resp,
    http.Client client,
  ) {
    final controller = StreamController<RelaySyncStreamEvent>();
    String? eventId;
    String? eventType;
    final dataLines = <String>[];

    void emit() {
      if (dataLines.isEmpty) {
        eventId = null;
        eventType = null;
        return;
      }
      final data = dataLines.join('\n').trim();
      dataLines.clear();
      final kind = (eventType ?? 'sync').trim();
      eventType = null;
      if (kind != 'sync' || data.isEmpty) {
        eventId = null;
        return;
      }
      try {
        final decoded = jsonDecode(data);
        if (decoded is! Map) return;
        final raw = decoded.cast<String, dynamic>();
        final withId = <String, dynamic>{
          ...raw,
          if ((raw['event_id'] == null || raw['event_id'] == 0) &&
              eventId != null)
            'event_id': int.tryParse(eventId!.trim()) ?? 0,
        };
        controller.add(RelaySyncStreamEvent.fromJson(withId));
      } catch (_) {
        // Ignore malformed stream chunks and keep the connection alive.
      } finally {
        eventId = null;
      }
    }

    final sub = resp.stream
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen((line) {
      if (line.isEmpty) {
        emit();
        return;
      }
      if (line.startsWith(':')) return;
      if (line.startsWith('id:')) {
        eventId = line.substring(3).trim();
        return;
      }
      if (line.startsWith('event:')) {
        eventType = line.substring(6).trim();
        return;
      }
      if (line.startsWith('data:')) {
        dataLines.add(line.substring(5).trimLeft());
      }
    }, onDone: () async {
      emit();
      await controller.close();
      client.close();
    }, onError: (Object e, StackTrace st) async {
      if (!controller.isClosed) {
        controller.addError(e, st);
        await controller.close();
      }
      client.close();
    }, cancelOnError: true);

    controller.onCancel = () async {
      await sub.cancel();
      client.close();
    };
    return controller.stream;
  }
}
