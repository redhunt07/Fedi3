/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core/core_api.dart';
import '../model/core_config.dart';

enum PresenceStatus { online, busy, afk, offline }

PresenceStatus presenceFromString(String? raw) {
  switch (raw?.trim().toLowerCase()) {
    case 'online':
      return PresenceStatus.online;
    case 'busy':
      return PresenceStatus.busy;
    case 'afk':
      return PresenceStatus.afk;
    default:
      return PresenceStatus.offline;
  }
}

String presenceStatusName(PresenceStatus status) => status.name;

class PeerPresenceStore {
  PeerPresenceStore._();

  static final PeerPresenceStore instance = PeerPresenceStore._();

  /// Presence keyed by Fedi3 username. Values expire after [ttl].
  final ValueNotifier<Map<String, PresenceStatus>> statusByUsername =
      ValueNotifier(const {});

  /// Status the local user selected. Kept in sync with [start]/[setSelfStatus].
  final ValueNotifier<PresenceStatus> selfStatus =
      ValueNotifier(PresenceStatus.offline);

  static const Duration ttl = Duration(seconds: 150);
  static const Duration pollInterval = Duration(seconds: 15);
  static const Duration heartbeatInterval = Duration(seconds: 60);

  CoreConfig? _config;
  Timer? _poll;
  Timer? _heartbeat;
  bool _loading = false;
  bool _setting = false;

  void start(CoreConfig config, {String selfStatus = 'online'}) {
    final sameConfig = _config?.publicBaseUrl == config.publicBaseUrl &&
        _config?.relayWs == config.relayWs;
    _config = config;
    this.selfStatus.value = presenceFromString(selfStatus);
    if (sameConfig && _poll != null) return;
    _poll?.cancel();
    _heartbeat?.cancel();
    _poll = Timer.periodic(pollInterval, (_) => refresh());
    _heartbeat = Timer.periodic(heartbeatInterval, (_) => _heartbeatTick());
    _publishSelf();
    refresh();
  }

  void stop() {
    _poll?.cancel();
    _poll = null;
    _heartbeat?.cancel();
    _heartbeat = null;
    _config = null;
    statusByUsername.value = const {};
  }

  Future<void> setSelfStatus(PresenceStatus status) async {
    selfStatus.value = status;
    final config = _config;
    if (config == null) return;
    if (_setting) return;
    _setting = true;
    try {
      await CoreApi(config: config).setPresence(presenceStatusName(status));
      await refresh();
    } catch (_) {
      // Best-effort: the local state still reflects the chosen status.
    } finally {
      _setting = false;
    }
  }

  Future<void> _publishSelf() async {
    final config = _config;
    if (config == null) return;
    try {
      await CoreApi(config: config)
          .setPresence(presenceStatusName(selfStatus.value));
    } catch (_) {
      // Ignore: the periodic heartbeat retries.
    }
  }

  Future<void> _heartbeatTick() async {
    await _publishSelf();
    await refresh();
  }

  Future<void> refresh() async {
    if (_loading) return;
    final config = _config;
    if (config == null) return;
    _loading = true;
    try {
      final api = CoreApi(config: config);
      final relay = await api.fetchRelayPeers(limit: 500);
      final presence = await api.fetchPresence();
      final next = <String, PresenceStatus>{};

      // Relay directory: online/offline fallback for Fedi3 accounts.
      final relayItems = relay['items'];
      if (relayItems is List) {
        for (final it in relayItems) {
          if (it is! Map) continue;
          final username = (it['username'] as String?)?.trim() ?? '';
          if (username.isEmpty) continue;
          next[username] =
              it['online'] == true ? PresenceStatus.online : PresenceStatus.offline;
        }
      }

      // Explicit presence sent by Fedi3 peers (cross-relay chat contacts).
      final now = DateTime.now().millisecondsSinceEpoch;
      final presenceItems = presence['items'];
      if (presenceItems is List) {
        for (final it in presenceItems) {
          if (it is! Map) continue;
          final actor = (it['actor_id'] as String?)?.trim() ?? '';
          final username = _usernameFromActor(actor);
          if (username.isEmpty) continue;
          final status = presenceFromString(it['status'] as String?);
          final updated = (it['updated_at_ms'] as num?)?.toInt() ?? 0;
          final fresh = now - updated <= ttl.inMilliseconds;
          if (!fresh) {
            if (!next.containsKey(username)) {
              next[username] = PresenceStatus.offline;
            }
            continue;
          }
          next[username] = status;
        }
      }

      final selfActor = (presence['self_actor'] as String?)?.trim() ?? '';
      final selfName = _usernameFromActor(selfActor);
      if (selfName.isNotEmpty) {
        next[selfName] = selfStatus.value;
      }

      if (!mapEquals(next, statusByUsername.value)) {
        statusByUsername.value = next;
      }
    } catch (_) {
      // Best-effort: keep existing state if refresh fails.
    } finally {
      _loading = false;
    }
  }

  PresenceStatus statusFor(String? username) {
    final key = username?.trim() ?? '';
    if (key.isEmpty) return PresenceStatus.offline;
    return statusByUsername.value[key] ?? PresenceStatus.offline;
  }

  static String _usernameFromActor(String actorUrl) {
    final uri = Uri.tryParse(actorUrl.trim());
    if (uri == null) return '';
    for (final seg in uri.pathSegments.reversed) {
      final value = seg.trim();
      if (value.isEmpty) continue;
      return value.startsWith('@') ? value.substring(1) : value;
    }
    return '';
  }
}
