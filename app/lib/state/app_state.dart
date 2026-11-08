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
    bool credentialsUnavailable = false,
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
        _credentialsUnavailable = credentialsUnavailable,
        _prefsCorrupt = prefsCorrupt;

  CoreConfig? _config;
  UiPrefs _prefs;
  int? _coreHandle;
  Future<void>? _stopFuture;
  Future<void> _prefsWrites = Future<void>.value();
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
  bool _credentialsUnavailable;
  bool _prefsCorrupt;

  /// Mobile platforms have no HOME, so the core cannot derive a writable data
  /// directory. The runner supplies the app support directory here.
  String? _coreDataDirBase;

  Timer? _watchdog;
  Future<void>? _starting;
  bool _restarting = false;
  int _restartAttempt = 0;
  DateTime? _restartAfter;
  bool _disposed = false;

  CoreConfig? get config => _config;
  UiPrefs get prefs => _prefs;
  int? get coreHandle => _coreHandle;
  bool get isRunning => _coreHandle != null;
  String? get lastError => _lastError;
  bool get configCorrupt => _configCorrupt;
  bool get credentialsUnavailable => _credentialsUnavailable;
  bool get prefsCorrupt => _prefsCorrupt;
  int get unreadNotifications => _unreadNotifications;
  int get unreadChats => _unreadChats;

  CoreRuntime get _runtime => _core ?? Fedi3Core.instance;

  @override
  void notifyListeners() {
    if (!_disposed) super.notifyListeners();
  }

  void markCoreDead([String? error]) {
    final handle = _coreHandle;
    _coreHandle = null;
    if (handle != null) {
      final operation = _releaseHandle(handle).catchError((Object e) {
        TelemetryService.record('core_stop_failed', e.toString());
      });
      late final Future<void> tracked;
      tracked = operation.whenComplete(() {
        if (identical(_stopFuture, tracked)) _stopFuture = null;
      });
      _stopFuture = tracked;
      unawaited(tracked);
    }
    _lastError = error;
    _unreadNotifications = 0;
    _unreadChats = 0;
    if (error != null && error.trim().isNotEmpty) {
      TelemetryService.record('core_dead', error);
    }
    notifyListeners();
  }

  Future<void> _releaseHandle(int handle) async {
    final runtime = _runtime;
    if (runtime is AsyncCoreStop) {
      await (runtime as AsyncCoreStop).stopAsync(handle);
    } else {
      runtime.stop(handle);
    }
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
    final configResult = await ConfigStore.loadSecure();
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
      credentialsUnavailable: configResult.isUnavailable,
      prefsCorrupt: prefsResult.corrupt,
    );
    if (configResult.isCorrupt || configResult.isUnavailable) {
      state._lastError = configResult.error;
    }
    return state;
  }

  Future<void> reloadConfig() async {
    final result = await ConfigStore.loadSecure();
    _configCorrupt = result.isCorrupt;
    _credentialsUnavailable = result.isUnavailable;
    _lastError = result.error;
    if (result.data != null) _config = CoreConfig.fromJson(result.data!);
    notifyListeners();
  }

  Future<void> saveConfig(CoreConfig cfg) async {
    // Atomic write first: if it fails, the previous config stays authoritative.
    await ConfigStore.writeSecure(cfg.toJson());
    _config = cfg;
    _configCorrupt = false;
    _credentialsUnavailable = false;
    notifyListeners();
  }

  /// Whole-snapshot replacement is reserved for explicit backup restore.
  Future<void> savePrefs(UiPrefs prefs) => _enqueuePrefs((_) => prefs);

  /// Evaluate UI mutations against the latest persisted snapshot, not a stale
  /// build() capture. Read cursors merge monotonically across concurrent chats.
  Future<void> updatePrefs(UiPrefs Function(UiPrefs) change) =>
      _enqueuePrefs((current) {
        final next = change(current);
        int latest(int a, int b) => a > b ? a : b;
        final threads = Map<String, int>.from(current.chatThreadSeenMs);
        next.chatThreadSeenMs.forEach((id, value) {
          threads[id] = latest(threads[id] ?? 0, value);
        });
        return next.copyWith(
          lastChatSeenMs: latest(current.lastChatSeenMs, next.lastChatSeenMs),
          lastNotificationsSeenMs: latest(
              current.lastNotificationsSeenMs, next.lastNotificationsSeenMs),
          chatThreadSeenMs: threads,
        );
      });

  Future<void> _enqueuePrefs(UiPrefs Function(UiPrefs) change) {
    final operation = _prefsWrites.then((_) async {
      final prev = _prefs;
      final next = change(prev);
      await PrefsStore.write(next);
      _prefs = next;
      _prefsCorrupt = false;
      notifyListeners();
      if ((prev.telemetryEnabled || prev.clientMonitoringEnabled) &&
          !(next.telemetryEnabled || next.clientMonitoringEnabled)) {
        await TelemetryService.clear();
      }
    });
    _prefsWrites =
        operation.then<void>((_) {}, onError: (Object _, StackTrace __) {});
    return operation;
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

  /// Points the core data directory at a writable app-private path.
  /// Only needed on platforms that expose no HOME (Android).
  void useCoreDataDirBase(String? path) => _coreDataDirBase = path;

  /// Starts the core. The FFI contract guarantees that a normal return means
  /// the server is listening, so the handle is stored only after that.
  Future<void> startCore() async {
    final stopping = _stopFuture;
    if (stopping != null) await stopping;
    final cfg = _config;
    if (cfg == null) return;
    if (_coreHandle != null) return;
    final pending = _starting;
    if (pending != null) return pending; // AP-06: one FFI call at a time
    if (!_restarting) {
      _restartAttempt = 0;
      _restartAfter = null;
    }

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

      final payload = effective.toCoreStartJson();
      final dataDirBase = _coreDataDirBase?.trim();
      if (dataDirBase != null && dataDirBase.isNotEmpty) {
        payload['data_dir'] =
            '${dataDirBase.replaceAll(RegExp(r'/+$'), '')}/${effective.username}';
      }
      final handle = _runtime.startJson(jsonEncode(payload));
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
      _startWatchdog();
    }
    notifyListeners();
  }

  Future<void> stopCore() async {
    final current = _stopFuture;
    if (current != null) return current;
    final operation = _stopCoreInner();
    _stopFuture = operation;
    try {
      await operation;
    } finally {
      _stopFuture = null;
    }
  }

  Future<void> _stopCoreInner() async {
    final handle = _coreHandle;
    _coreHandle = null;
    _stopWatchdog();
    _lastError = null;
    if (handle == null) {
      _coreHandle = null;
      notifyListeners();
      return;
    }
    try {
      await _releaseHandle(handle);
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
    if (_restartAttempt >= _restartMaxAttempts) {
      _restartAfter ??= DateTime.now().add(const Duration(minutes: 5));
      if (DateTime.now().isBefore(_restartAfter!)) return;
      _restartAttempt = 0;
      _restartAfter = null;
    }

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
    final handle = _coreHandle;
    _coreHandle = null;
    if (handle != null) {
      unawaited(_releaseHandle(handle).catchError((Object e) {
        TelemetryService.record('core_stop_failed', e.toString());
      }));
    }
    _disposed = true;
    _stopWatchdog();
    super.dispose();
  }
}
