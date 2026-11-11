/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;

import '../model/core_config.dart';

/// Lightweight background wake source for the push-only keep-alive service.
///
/// It polls the relay's chat spool cursor over HTTPS and asks the service to
/// start the local core only when new items exist. The core then syncs and
/// shows the real notifications (chat preview, calls, activity) and is stopped
/// again after a short quiet period, so no Rust core and no P2P swarm stay
/// alive while the phone is idle.
class RelaySpoolPoller {
  RelaySpoolPoller({
    required this.config,
    required this.directory,
    required this.onNewItems,
    this.interval = const Duration(seconds: 15),
    http.Client? client,
  }) : _client = client ?? http.Client();

  final CoreConfig config;
  final Directory directory;
  final Future<void> Function() onNewItems;
  final Duration interval;
  final http.Client _client;

  Timer? _timer;
  bool _started = false;
  bool _polling = false;
  int _lastId = 0;

  static const _cursorFile = 'relay_spool.json';

  bool get enabled =>
      config.relayWs.trim().isNotEmpty && config.relayToken.trim().isNotEmpty;

  Future<void> start() async {
    if (_started || !enabled) return;
    _started = true;
    _lastId = await _readCursor();
    await _poll();
    _timer = Timer.periodic(interval, (_) => unawaited(_poll()));
  }

  Future<void> stop() async {
    _started = false;
    _timer?.cancel();
    _timer = null;
    _client.close();
  }

  Uri _endpoint() {
    final ws = Uri.parse(config.relayWs.trim());
    final scheme = switch (ws.scheme) {
      'wss' || 'https' => 'https',
      'ws' || 'http' => 'http',
      _ => 'https',
    };
    return Uri(
      scheme: scheme,
      host: ws.host,
      port: ws.hasPort ? ws.port : null,
      path: '/_fedi3/chat/spool',
      queryParameters: {
        'user': config.username,
        'after': '$_lastId',
        'limit': '50',
      },
    );
  }

  Future<void> _poll() async {
    if (_polling || !_started) return;
    _polling = true;
    try {
      final response = await _client.get(
        _endpoint(),
        headers: {
          'Authorization': 'Bearer ${config.relayToken}',
          'Accept': 'application/json',
        },
      ).timeout(const Duration(seconds: 15));
      if (response.statusCode != 200) {
        debugPrint('[spool] poll status ${response.statusCode}');
        return;
      }
      final decoded = jsonDecode(response.body);
      if (decoded is! Map) return;
      final latest = decoded['latest_id'];
      final latestId = latest is num ? latest.toInt() : _lastId;
      final items = decoded['items'];
      final hasItems = items is List && items.isNotEmpty;
      if (latestId > _lastId) {
        _lastId = latestId;
        await _writeCursor(latestId);
      }
      debugPrint(
        '[spool] status=${response.statusCode} after=$_lastId '
        'latest=$latestId items=${items is List ? items.length : -1}',
      );
      if (hasItems) {
        await onNewItems();
      }
    } catch (e) {
      debugPrint('[spool] poll failed: $e');
    } finally {
      _polling = false;
    }
  }

  Future<int> _readCursor() async {
    try {
      final file =
          File('${directory.path}${Platform.pathSeparator}$_cursorFile');
      if (!await file.exists()) return 0;
      final raw = jsonDecode(await file.readAsString());
      if (raw is Map && raw['last_id'] is num) {
        return (raw['last_id'] as num).toInt();
      }
    } catch (_) {}
    return 0;
  }

  Future<void> _writeCursor(int id) async {
    try {
      final file =
          File('${directory.path}${Platform.pathSeparator}$_cursorFile');
      await file.writeAsString(jsonEncode({'last_id': id}));
    } catch (_) {}
  }
}
