/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../core/core_api.dart';
import '../core/fedi3_core.dart';
import '../model/core_config.dart';
import '../model/ui_prefs.dart';
import '../services/telemetry_service.dart';
import 'config_store.dart';
import 'peer_presence_store.dart';
import 'prefs_store.dart';

/// Probes `/_fedi3/health` (C12/H-51). Injectable for tests.
typedef CoreHealthCheck = Future<bool> Function(CoreConfig config);

class AppState extends ChangeNotifier {
  AppState({
    required CoreConfig? config,
    required UiPrefs prefs,
    CoreRuntime? core,
    CoreHealthCheck? healthCheck,
    Duration? watchdogInterval = const Duration(seconds: 5),
    bool watchdogEnabled = true,
    Duration restartBaseDelay = const Duration(seconds: 2),
    int restartMaxAttempts = 5,
    bool configCorrupt = false,
    bool prefsCorrupt = false,
  })  : _config = config,
        _prefs = prefs,
        _core = core,
        _healthCheck = healthCheck,
        _watchdogInterval = watchdogInterval,
        _watchdogEnabled = watchdogEnabled,
        _restartBaseDelay = restartBaseDelay,
        _restartMaxAttempts = restartMaxAttempts,
        _configCorrupt = configCorrupt,
        _prefsCorrupt = prefsCorrupt;

  CoreConfig? _config;
  UiPrefs _prefs;
  int? _coreHandle;
  String? _lastError;
  int _unreadNotifications = 0;
  int _unreadChats = 0;

  final CoreRuntime? _core;
  final CoreHealthCheck? _healthCheck;
  final Duration? _watchdogInterval;
  final bool _watchdogEnabled;
  final Duration _restartBaseDelay;
  final int _restartMaxAttempts;
  bool _configCorrupt;
  bool _prefsCorrupt;

  Timer? _watchdog;
  Future<void>? _starting;
  bool _restarting = false;
  int _restartAttempt = 0;
  bool _disposed = false;

  CoreConfig? get config => _config;
  UiPrefs get prefs => _prefs;
  int? get coreHandle => _coreHandle;
  bool get isRunning => _coreHandle != null;
  String? get lastError => _lastError;
  bool get configCorrupt => _configCorrupt;
  bool get prefsCorrupt => _prefsCorrupt;
  int get unreadNotifications => _unreadNotifications;
  int get unreadChats => _unreadChats;

  CoreRuntime get _runtime => _core ?? Fedi3Core.instance;

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

  static Future<AppState> load({
    CoreRuntime? core,
    CoreHealthCheck? healthCheck,
    Duration? watchdogInterval,
    bool watchdogEnabled = true,
  }) async {
    final configResult = ConfigStore.load();
    final prefsResult = await PrefsStore.load();
    final cfgData = configResult.data;
    final state = AppState(
      config: cfgData == null ? null : CoreConfig.fromJson(cfgData),
      prefs: prefsResult.prefs,
      core: core,
      healthCheck: healthCheck,
      watchdogInterval: watchdogInterval,
      watchdogEnabled: watchdogEnabled,
      configCorrupt: configResult.isCorrupt,
      prefsCorrupt: prefsResult.corrupt,
    );
    if (configResult.isCorrupt) {
      state._lastError = configResult.error;
    }
    return state;
  }

  Future<void> saveConfig(CoreConfig cfg) async {
    // Atomic write first: if it fails, the previous config stays authoritative.
    ConfigStore.writeConfig(cfg.toJson());
    _config = cfg;
    _configCorrupt = false;
    notifyListeners();
  }

  Future<void> savePrefs(UiPrefs prefs) async {
    final prev = _prefs;
    await PrefsStore.write(prefs);
    _prefs = prefs;
    _prefsCorrupt = false;
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

  // ---------------------------------------------------------------------------
  // Core lifecycle (AP-01..06, P0-12/D3).
  // ---------------------------------------------------------------------------

  /// Starts the core. The FFI contract guarantees that a normal return means
  /// the server is listening, so the handle is stored only after that.
  Future<void> startCore() async {
    final cfg = _config;
    if (cfg == null) return;
    if (_coreHandle != null) return;
    final pending = _starting;
    if (pending != null) return pending; // AP-06: one FFI call at a time

    final future = _startCoreInternal(cfg);
    _starting = future;
    try {
      await future;
    } finally {
      _starting = null;
    }
  }

  Future<void> _startCoreInternal(CoreConfig cfg) async {
    _lastError = null;
    try {
      var effective = cfg;

      String normalizeRelayWs(String v) {
        var s = v.trim().replaceAll(RegExp(r'/+$'), '');
        if (s.startsWith('https://')) {
          return 'wss://${s.substring('https://'.length)}';
        }
        if (s.startsWith('http://')) {
          return 'ws://${s.substring('http://'.length)}';
        }
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
        effective = effective.copyWith(relayWs: normalizedRelayWs);
        await saveConfig(effective);
      }

      if (!(effective.relayWs.startsWith('ws://') ||
          effective.relayWs.startsWith('wss://'))) {
        throw StateError(
            'Relay WS must start with ws:// or wss:// (got: ${effective.relayWs})');
      }

      if (effective.relayToken.trim().length < 16) {
        final prev = effective.relayToken.trim();
        effective = effective.copyWith(
          relayToken: CoreConfig.randomToken(),
          previousRelayToken:
              prev.isEmpty ? effective.previousRelayToken : prev,
        );
        await saveConfig(effective);
      }
      if (effective.internalToken.trim().isEmpty) {
        effective = effective.copyWith(internalToken: CoreConfig.randomToken());
        await saveConfig(effective);
      }

      final handle =
          _runtime.startJson(jsonEncode(effective.toCoreStartJson()));
      if (handle <= 0) {
        throw StateError('core start returned invalid handle ($handle)');
      }
      _coreHandle = handle;
      _restartAttempt = 0;
      _startWatchdog();
    } catch (e) {
      // AP-02/P0-10: never keep a stale handle when FFI throws.
      _coreHandle = null;
      _lastError = e.toString();
      TelemetryService.record(
          'core_start_failed', _lastError ?? 'unknown error');
    }
    notifyListeners();
  }

  Future<void> stopCore() async {
    final handle = _coreHandle;
    _stopWatchdog();
    _lastError = null;
    if (handle == null) {
      _coreHandle = null;
      notifyListeners();
      return;
    }
    try {
      _runtime.stop(handle);
    } catch (e) {
      _lastError = e.toString();
      TelemetryService.record(
          'core_stop_failed', _lastError ?? 'unknown error');
    } finally {
      // AP-03: the handle is always cleared, even if the native stop failed.
      _coreHandle = null;
    }
    notifyListeners();
  }

  /// P0-12/D3/AP-05: stop → persist → start. On failure the previous config is
  /// restored and the core is restarted, so the UI never stays empty.
  Future<bool> replaceConfig(CoreConfig cfg) async {
    final previous = _config;
    await stopCore();
    await saveConfig(cfg);
    await startCore();
    if (_coreHandle != null) return true;

    final failure = _lastError ?? 'core failed to start';
    if (previous != null) {
      try {
        await saveConfig(previous);
        await startCore();
      } catch (e) {
        _lastError = '$failure; rollback failed: $e';
      }
    }
    throw StateError(_lastError ?? failure);
  }

  // ---------------------------------------------------------------------------
  // Watchdog (C12/H-51/AP-04).
  // ---------------------------------------------------------------------------

  void _startWatchdog() {
    _stopWatchdog();
    if (!_watchdogEnabled || _watchdogInterval == null) return;
    _watchdog = Timer.periodic(_watchdogInterval, (_) {
      unawaited(runWatchdogOnce());
    });
  }

  void _stopWatchdog() {
    _watchdog?.cancel();
    _watchdog = null;
  }

  Future<bool> _probeHealth(CoreConfig cfg) async {
    final check = _healthCheck;
    if (check != null) {
      try {
        return await check(cfg);
      } catch (_) {
        return false;
      }
    }
    return CoreApi(config: cfg).pingHealth();
  }

  /// One watchdog tick. Exposed so tests can drive it deterministically.
  @visibleForTesting
  Future<void> runWatchdogOnce() async {
    if (_disposed) return;
    final cfg = _config;
    if (cfg == null) return;

    if (_coreHandle == null) {
      await _attemptRestart();
      return;
    }
    final healthy = await _probeHealth(cfg);
    if (healthy) {
      _restartAttempt = 0;
      return;
    }
    markCoreDead('core health check failed');
    await _attemptRestart();
  }

  Future<void> _attemptRestart() async {
    if (_disposed || !_watchdogEnabled || _restarting) return;
    if (_config == null) return;
    if (_restartAttempt >= _restartMaxAttempts) return;

    _restarting = true;
    try {
      final attempt = _restartAttempt + 1;
      _restartAttempt = attempt;
      final multiplier = 1 << (attempt - 1);
      final delay = _restartBaseDelay * multiplier;
      if (delay > Duration.zero) {
        await Future<void>.delayed(delay);
      }
      if (_disposed) return;
      await startCore();
    } finally {
      _restarting = false;
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _stopWatchdog();
    super.dispose();
  }
}
