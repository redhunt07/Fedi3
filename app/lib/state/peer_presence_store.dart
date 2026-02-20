/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core/core_api.dart';
import '../model/core_config.dart';

class PeerPresenceStore {
  PeerPresenceStore._();

  static final PeerPresenceStore instance = PeerPresenceStore._();

  final ValueNotifier<Map<String, bool>> onlineByUsername = ValueNotifier(const {});

  CoreConfig? _config;
  Timer? _timer;
  StreamSubscription<Map<String, dynamic>>? _presenceSub;
  bool _loading = false;

  void start(CoreConfig config) {
    final sameConfig = _config?.publicBaseUrl == config.publicBaseUrl && _config?.relayWs == config.relayWs;
    _config = config;
    if (sameConfig && (_timer != null || _presenceSub != null)) return;
    _timer?.cancel();
    _timer = null;
    _presenceSub?.cancel();
    _presenceSub = null;
    onlineByUsername.value = const {};
    _startPresenceStream();
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
    _presenceSub?.cancel();
    _presenceSub = null;
    _config = null;
    onlineByUsername.value = const {};
  }

  void _startPresenceStream() {
    final config = _config;
    if (config == null) return;
    final api = CoreApi(config: config);
    _presenceSub = api.relayPresenceStream().listen(
      _handlePresenceEvent,
      onError: (_) => _ensurePollingFallback(),
      onDone: _ensurePollingFallback,
    );
  }

  void _ensurePollingFallback() {
    if (_timer != null) return;
    _timer = Timer.periodic(const Duration(seconds: 3), (_) => refresh());
    refresh();
  }

  void _handlePresenceEvent(Map<String, dynamic> event) {
    final eventType = (event['event'] as String?)?.trim().toLowerCase() ?? '';
    if (eventType == 'snapshot') {
      _applySnapshot(event['items']);
      return;
    }
    if (eventType == 'update') {
      _applyUpdate(event);
      return;
    }
    if (eventType == 'message') {
      if (event.containsKey('items')) {
        _applySnapshot(event['items']);
      } else if (event.containsKey('username')) {
        _applyUpdate(event);
      } else if (event.containsKey('item')) {
        _applyUpdate(event['item']);
      }
    }
  }

  void _applySnapshot(Object? items) {
    if (items is! List) return;
    final next = <String, bool>{};
    for (final it in items) {
      _mergePresenceItem(next, it);
    }
    if (!mapEquals(next, onlineByUsername.value)) {
      onlineByUsername.value = next;
    }
  }

  void _applyUpdate(Object? item) {
    if (item is! Map) return;
    final next = Map<String, bool>.from(onlineByUsername.value);
    _mergePresenceItem(next, item);
    if (!mapEquals(next, onlineByUsername.value)) {
      onlineByUsername.value = next;
    }
  }

  void _mergePresenceItem(Map<String, bool> map, Object? raw) {
    if (raw is! Map) return;
    final username = (raw['username'] as String?)?.trim().toLowerCase() ?? '';
    if (username.isEmpty) return;
    final online = raw['online'] == true;
    map[username] = online;
    final actorUrl = (raw['actor_url'] as String?)?.trim().toLowerCase() ?? '';
    if (actorUrl.isNotEmpty) {
      map[actorUrl] = online;
      final uri = Uri.tryParse(actorUrl);
      if (uri != null && uri.host.isNotEmpty) {
        map['$username@${uri.host.toLowerCase()}'] = online;
      }
    }
  }

  Future<void> refresh() async {
    if (_loading) return;
    final config = _config;
    if (config == null) return;
    _loading = true;
    try {
      final api = CoreApi(config: config);
      final resp = await api.fetchRelayPeers(limit: 500);
      final items = resp['items'];
      final next = <String, bool>{};
      if (items is List) {
        for (final it in items) {
          if (it is! Map) continue;
          final username = (it['username'] as String?)?.trim().toLowerCase() ?? '';
          if (username.isEmpty) continue;
          final online = it['online'] == true;
          next[username] = online;
          final actorUrl = (it['actor_url'] as String?)?.trim().toLowerCase() ?? '';
          if (actorUrl.isNotEmpty) {
            next[actorUrl] = online;
            final uri = Uri.tryParse(actorUrl);
            if (uri != null && uri.host.isNotEmpty) {
              next['$username@${uri.host.toLowerCase()}'] = online;
            }
          }
        }
      }
      if (!mapEquals(next, onlineByUsername.value)) {
        onlineByUsername.value = next;
      }
    } catch (_) {
      // Best-effort: keep existing state if refresh fails.
    } finally {
      _loading = false;
    }
  }
}
