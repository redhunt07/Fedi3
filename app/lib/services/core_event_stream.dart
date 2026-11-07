/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import '../model/core_config.dart';
import '../core/core_http.dart';
import 'app_activity.dart';

class CoreEvent {
  CoreEvent({
    required this.kind,
    required this.tsMs,
    this.activityType,
    this.activityId,
  });

  final String kind;
  final int tsMs;
  final String? activityType;
  final String? activityId;

  static CoreEvent? tryParse(Map<String, dynamic> json) {
    final kind = json['kind']?.toString().trim() ?? '';
    if (kind.isEmpty) return null;
    final ts = (json['ts_ms'] is num)
        ? (json['ts_ms'] as num).toInt()
        : int.tryParse(json['ts_ms']?.toString() ?? '') ?? 0;
    return CoreEvent(
      kind: kind,
      tsMs: ts,
      activityType: json['activity_type']?.toString(),
      activityId: json['activity_id']?.toString(),
    );
  }
}

class CoreEventStream {
  CoreEventStream({required this.config, http.Client? client})
      : _client = client;

  final CoreConfig config;
  final http.Client? _client;

  /// Opens the SSE stream.
  ///
  /// H-50/AN-08..10: the connection has a bounded timeout, malformed frames are
  /// dropped without killing the stream, and network errors propagate so the
  /// caller can reconnect with backoff.
  Stream<CoreEvent> stream({String? kind}) async* {
    while (true) {
      await AppActivity.waitForForeground();
      final client = _client ?? CoreHttp.streamClient();
      final lifecycle = AppActivity.changes.listen((foreground) {
        if (!foreground &&
            _client == null &&
            CoreHttp.ownsStreamClient(client)) {
          client.close();
        }
      });
      try {
        yield* _connection(client, kind: kind);
      } catch (_) {
        if (AppActivity.foreground) rethrow;
      } finally {
        await lifecycle.cancel();
        if (_client == null && CoreHttp.ownsStreamClient(client)) {
          client.close();
        }
      }
      if (AppActivity.foreground) return;
    }
  }

  Stream<CoreEvent> _connection(http.Client client, {String? kind}) async* {
    final base = config.localBaseUri;
    final uri = base.replace(
      path: '/_fedi3/stream',
      queryParameters: {
        if (kind != null && kind.trim().isNotEmpty) 'kind': kind.trim(),
      },
    );

    final req = http.Request('GET', uri);
    if (config.internalToken.trim().isNotEmpty) {
      req.headers['X-Fedi3-Internal'] = config.internalToken.trim();
    }
    req.headers['Accept'] = 'text/event-stream';

    final resp = await client.send(req).timeout(CoreHttp.totalTimeout);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('stream failed: ${resp.statusCode}');
    }

    var dataBuf = '';
    await for (final line in resp.stream
        .transform(utf8.decoder)
        .transform(const LineSplitter())) {
      if (line.startsWith('data:')) {
        dataBuf += line.substring('data:'.length).trimLeft();
        continue;
      }
      if (line.isEmpty) {
        if (dataBuf.isNotEmpty) {
          final ev = _tryParseFrame(dataBuf);
          if (ev != null) yield ev;
          dataBuf = '';
        }
        continue;
      }
    }
  }

  CoreEvent? _tryParseFrame(String data) {
    try {
      final json = jsonDecode(data);
      if (json is! Map) return null;
      return CoreEvent.tryParse(json.cast<String, dynamic>());
    } on FormatException {
      return null;
    } catch (_) {
      return null;
    }
  }
}
