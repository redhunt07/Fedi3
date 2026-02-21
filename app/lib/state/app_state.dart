/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../core/fedi3_core.dart';
import '../model/core_config.dart';
import '../model/ui_prefs.dart';
import '../services/telemetry_service.dart';
import '../services/cloud_backup_service.dart';
import 'config_store.dart';
import 'peer_presence_store.dart';
import 'prefs_store.dart';

class AppState extends ChangeNotifier {
  AppState({required CoreConfig? config, required UiPrefs prefs})
      : _config = config,
        _prefs = prefs;

  CoreConfig? _config;
  UiPrefs _prefs;
  int? _coreHandle;
  String? _lastError;
  int _unreadNotifications = 0;
  int _unreadChats = 0;
  Timer? _cloudBackupTimer;
  bool _cloudBackupRunning = false;

  CoreConfig? get config => _config;
  UiPrefs get prefs => _prefs;
  int? get coreHandle => _coreHandle;
  bool get isRunning => _coreHandle != null;
  String? get lastError => _lastError;
  int get unreadNotifications => _unreadNotifications;
  int get unreadChats => _unreadChats;

  void markCoreDead([String? error]) {
    _coreHandle = null;
    _lastError = error;
    _unreadNotifications = 0;
    _unreadChats = 0;
    if (error != null && error.trim().isNotEmpty) {
      TelemetryService.record('core_dead', error);
    }
    notifyListeners();
  }

  void incrementUnreadNotifications([int delta = 1]) {
    _unreadNotifications = (_unreadNotifications + delta).clamp(0, 1 << 30);
    notifyListeners();
  }

  void clearUnreadNotifications() {
    if (_unreadNotifications == 0) return;
    _unreadNotifications = 0;
    notifyListeners();
  }

  void incrementUnreadChats([int delta = 1]) {
    _unreadChats = (_unreadChats + delta).clamp(0, 1 << 30);
    notifyListeners();
  }

  void setUnreadChats(int value) {
    final next = value.clamp(0, 1 << 30);
    if (_unreadChats == next) return;
    _unreadChats = next;
    notifyListeners();
  }

  void clearUnreadChats() {
    if (_unreadChats == 0) return;
    _unreadChats = 0;
    notifyListeners();
  }

  static Future<AppState> load() async {
    final map = ConfigStore.readConfig();
    final prefs = await PrefsStore.read();
    return AppState(config: map == null ? null : CoreConfig.fromJson(map), prefs: prefs);
  }

  Future<void> saveConfig(CoreConfig cfg) async {
    _config = cfg;
    ConfigStore.writeConfig(cfg.toJson());
    notifyListeners();
  }

  Future<void> savePrefs(UiPrefs prefs) async {
    final prev = _prefs;
    _prefs = prefs;
    await PrefsStore.write(prefs);
    notifyListeners();
    if ((prev.telemetryEnabled || prev.clientMonitoringEnabled) &&
        !(prefs.telemetryEnabled || prefs.clientMonitoringEnabled)) {
      await TelemetryService.clear();
    }
  }

  Future<void> clearConfig() async {
    _config = null;
    ConfigStore.clear();
    PeerPresenceStore.instance.stop();
    notifyListeners();
  }

  Future<void> startCore() async {
    final cfg = _config;
    if (cfg == null) return;
    if (_coreHandle != null) return;
    _lastError = null;
    try {
      var effective = cfg;

      String normalizeRelayWs(String v) {
        var s = v.trim().replaceAll(RegExp(r'/+$'), '');
        if (s.startsWith('https://')) return 'wss://${s.substring('https://'.length)}';
        if (s.startsWith('http://')) return 'ws://${s.substring('http://'.length)}';
        if (s.startsWith('ws://')) {
          final pb = effective.publicBaseUrl.trim();
          if (pb.startsWith('https://')) {
            return 'wss://${s.substring('ws://'.length)}';
          }
        }
        return s;
      }

        final normalizedRelayWs = normalizeRelayWs(effective.relayWs);
        if (normalizedRelayWs != effective.relayWs.trim()) {
          effective = CoreConfig(
            username: effective.username,
            domain: effective.domain,
            publicBaseUrl: effective.publicBaseUrl,
            relayWs: normalizedRelayWs,
            relayToken: effective.relayToken,
            bind: effective.bind,
            internalToken: effective.internalToken,
            apRelays: effective.apRelays,
            bootstrapFollowActors: effective.bootstrapFollowActors,
            displayName: effective.displayName,
            summary: effective.summary,
            iconUrl: effective.iconUrl,
            iconMediaType: effective.iconMediaType,
            imageUrl: effective.imageUrl,
            imageMediaType: effective.imageMediaType,
            profileFields: effective.profileFields,
            manuallyApprovesFollowers: effective.manuallyApprovesFollowers,
            blockedDomains: effective.blockedDomains,
            blockedActors: effective.blockedActors,
            previousPublicBaseUrl: effective.previousPublicBaseUrl,
            previousRelayToken: effective.previousRelayToken,
            upnpPortRangeStart: effective.upnpPortRangeStart,
            upnpPortRangeEnd: effective.upnpPortRangeEnd,
            upnpLeaseSecs: effective.upnpLeaseSecs,
            upnpTimeoutSecs: effective.upnpTimeoutSecs,
            postDeliveryMode: effective.postDeliveryMode,
            p2pRelayFallbackSecs: effective.p2pRelayFallbackSecs,
            p2pCacheTtlSecs: effective.p2pCacheTtlSecs,
          );
          await saveConfig(effective);
        }

      if (!(effective.relayWs.startsWith('ws://') || effective.relayWs.startsWith('wss://'))) {
        throw StateError('Relay WS must start with ws:// or wss:// (got: ${effective.relayWs})');
      }

      if (effective.relayToken.trim().length < 16) {
        final prev = effective.relayToken.trim();
        effective = CoreConfig(
          username: effective.username,
          domain: effective.domain,
          publicBaseUrl: effective.publicBaseUrl,
          relayWs: effective.relayWs,
          relayToken: CoreConfig.randomToken(),
          bind: effective.bind,
          internalToken: effective.internalToken,
          apRelays: effective.apRelays,
          bootstrapFollowActors: effective.bootstrapFollowActors,
          displayName: effective.displayName,
          summary: effective.summary,
          iconUrl: effective.iconUrl,
          iconMediaType: effective.iconMediaType,
          imageUrl: effective.imageUrl,
          imageMediaType: effective.imageMediaType,
          profileFields: effective.profileFields,
          manuallyApprovesFollowers: effective.manuallyApprovesFollowers,
          blockedDomains: effective.blockedDomains,
            blockedActors: effective.blockedActors,
            previousPublicBaseUrl: effective.previousPublicBaseUrl,
            previousRelayToken: prev.isEmpty ? effective.previousRelayToken : prev,
            upnpPortRangeStart: effective.upnpPortRangeStart,
            upnpPortRangeEnd: effective.upnpPortRangeEnd,
            upnpLeaseSecs: effective.upnpLeaseSecs,
            upnpTimeoutSecs: effective.upnpTimeoutSecs,
            postDeliveryMode: effective.postDeliveryMode,
            p2pRelayFallbackSecs: effective.p2pRelayFallbackSecs,
            p2pCacheTtlSecs: effective.p2pCacheTtlSecs,
          );
        await saveConfig(effective);
      }
      if (effective.internalToken.trim().isEmpty) {
        effective = CoreConfig(
          username: effective.username,
          domain: effective.domain,
          publicBaseUrl: effective.publicBaseUrl,
          relayWs: effective.relayWs,
          relayToken: effective.relayToken,
          bind: effective.bind,
          internalToken: CoreConfig.randomToken(),
          apRelays: effective.apRelays,
          bootstrapFollowActors: effective.bootstrapFollowActors,
          displayName: effective.displayName,
          summary: effective.summary,
          iconUrl: effective.iconUrl,
          iconMediaType: effective.iconMediaType,
          imageUrl: effective.imageUrl,
          imageMediaType: effective.imageMediaType,
          profileFields: effective.profileFields,
          manuallyApprovesFollowers: effective.manuallyApprovesFollowers,
          blockedDomains: effective.blockedDomains,
          blockedActors: effective.blockedActors,
          previousPublicBaseUrl: effective.previousPublicBaseUrl,
          previousRelayToken: effective.previousRelayToken,
          upnpPortRangeStart: effective.upnpPortRangeStart,
          upnpPortRangeEnd: effective.upnpPortRangeEnd,
          upnpLeaseSecs: effective.upnpLeaseSecs,
          upnpTimeoutSecs: effective.upnpTimeoutSecs,
          postDeliveryMode: effective.postDeliveryMode,
          p2pRelayFallbackSecs: effective.p2pRelayFallbackSecs,
          p2pCacheTtlSecs: effective.p2pCacheTtlSecs,
        );
        await saveConfig(effective);
      }

      final handle = Fedi3Core.instance.startJson(jsonEncode(effective.toCoreStartJson()));
      _coreHandle = handle;
      _scheduleCloudBackup();
    } catch (e) {
      _lastError = e.toString();
      TelemetryService.record('core_start_failed', _lastError ?? 'unknown error');
    }
    notifyListeners();
  }

  Future<void> stopCore() async {
    final handle = _coreHandle;
    if (handle == null) return;
    _lastError = null;
    try {
      Fedi3Core.instance.stop(handle);
      _coreHandle = null;
      _cloudBackupTimer?.cancel();
      _cloudBackupTimer = null;
    } catch (e) {
      _lastError = e.toString();
      TelemetryService.record('core_stop_failed', _lastError ?? 'unknown error');
    }
    notifyListeners();
  }

  void _scheduleCloudBackup() {
    _cloudBackupTimer?.cancel();
    final cfg = _config;
    if (cfg == null) return;
    _cloudBackupTimer = Timer.periodic(const Duration(hours: 6), (_) {
      _runCloudBackup();
    });
    Timer(const Duration(minutes: 2), _runCloudBackup);
  }

  Future<void> _runCloudBackup() async {
    if (_cloudBackupRunning) return;
    final cfg = _config;
    if (cfg == null || _coreHandle == null) return;
    _cloudBackupRunning = true;
    try {
      final service = CloudBackupService(config: cfg);
      await service.upload(prefs: _prefs);
      TelemetryService.record('cloud_backup_ok', cfg.username);
    } catch (e) {
      TelemetryService.record('cloud_backup_failed', e.toString());
    } finally {
      _cloudBackupRunning = false;
    }
  }
}
