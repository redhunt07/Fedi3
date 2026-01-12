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
  bool _loading = false;

  void start(CoreConfig config) {
    final sameConfig = _config?.publicBaseUrl == config.publicBaseUrl && _config?.relayWs == config.relayWs;
    _config = config;
    if (sameConfig && _timer != null) return;
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(seconds: 10), (_) => refresh());
    refresh();
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
    _config = null;
    onlineByUsername.value = const {};
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
          final username = (it['username'] as String?)?.trim() ?? '';
          if (username.isEmpty) continue;
          next[username] = it['online'] == true;
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
