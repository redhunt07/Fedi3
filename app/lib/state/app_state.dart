/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../core/core_api.dart';
import '../core/fedi3_core.dart';
import '../model/chat_models.dart';
import '../model/core_config.dart';
import '../model/relay_sync_models.dart';
import '../model/ui_prefs.dart';
import '../services/relay_sync_api.dart';
import '../services/relay_sync_stream_client.dart';
import '../services/secret_store.dart';
import '../services/telemetry_service.dart';
import '../services/cloud_backup_service.dart';
import 'config_store.dart';
import 'peer_presence_store.dart';
import 'prefs_store.dart';
import 'relay_sync_store.dart';

class AppState extends ChangeNotifier {
  AppState({required CoreConfig? config, required UiPrefs prefs})
      : _config = config,
        _prefs = prefs;

  CoreConfig? _config;
  UiPrefs _prefs;
  int? _coreHandle;
  bool _externalCore = false;
  String? _lastError;
  int _unreadNotifications = 0;
  int _unreadChats = 0;
  Timer? _cloudBackupTimer;
  bool _cloudBackupRunning = false;
  Timer? _relaySyncTimer;
  Timer? _relaySyncStreamRetry;
  Timer? _relaySyncStreamDebounce;
  StreamSubscription<RelaySyncStreamEvent>? _relaySyncStreamSub;
  bool _relaySyncStreamConnected = false;
  int _relaySyncStreamLastEventId = 0;
  bool _relaySyncReady = false;
  bool _relaySyncBusy = false;
  String? _relaySyncError;
  int _relaySyncLastSuccessMs = 0;
  int _relaySyncBootstrapFailCount = 0;
  int _relaySyncBootstrapRetryNotBeforeMs = 0;
  SyncCursorState _relaySyncCursors = SyncCursorState.empty;
  List<Map<String, dynamic>> _relayEvents = const [];
  List<Map<String, dynamic>> _relayTimelineHome = const [];
  List<Map<String, dynamic>> _relayNotifications = const [];
  List<Map<String, dynamic>> _relayChatEntries = const [];
  List<ChatThreadItem> _relayChatThreads = const [];

  CoreConfig? get config => _config;
  UiPrefs get prefs => _prefs;
  int? get coreHandle => _coreHandle;
  bool get isRunning => _coreHandle != null;
  bool get isExternalCore => _externalCore;
  String? get lastError => _lastError;
  int get unreadNotifications => _unreadNotifications;
  int get unreadChats => _unreadChats;
  bool get relaySyncReady => _relaySyncReady;
  bool get relaySyncBusy => _relaySyncBusy;
  bool get relaySyncStreamConnected => _relaySyncStreamConnected;
  String? get relaySyncError => _relaySyncError;
  int get relaySyncLastSuccessMs => _relaySyncLastSuccessMs;
  SyncCursorState get relaySyncCursors => _relaySyncCursors;
  List<Map<String, dynamic>> get relayEvents => _relayEvents;
  List<Map<String, dynamic>> get relayTimelineHome => _relayTimelineHome;
  List<Map<String, dynamic>> get relayNotifications => _relayNotifications;
  List<Map<String, dynamic>> get relayChatEntries => _relayChatEntries;
  List<ChatThreadItem> get relayChatThreads => _relayChatThreads;

  void markCoreDead([String? error]) {
    _coreHandle = null;
    _externalCore = false;
    _lastError = error;
    _updateRelayDerivedState();
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

  Future<void> applyRelayChatMessageDeleted({
    required String threadId,
    required String messageId,
  }) async {
    _applyRelayChatMessageDeletedLocal(threadId: threadId, messageId: messageId);
    _updateRelayDerivedState();
    notifyListeners();
    await _persistRelaySnapshot();
  }

  Future<void> applyRelayChatThreadDeleted({required String threadId}) async {
    _applyRelayChatThreadDeletedLocal(threadId: threadId);
    _updateRelayDerivedState();
    notifyListeners();
    await _persistRelaySnapshot();
  }

  static Future<AppState> load() async {
    final configRaw = ConfigStore.readConfigRaw();
    final prefsRaw = await PrefsStore.readRaw();
    await SecretStore.migrateLegacy(configRaw: configRaw, prefsRaw: prefsRaw);
    final prefs = await SecretStore.hydratePrefs(await PrefsStore.read());
    final config = configRaw == null ? null : await SecretStore.hydrateConfig(CoreConfig.fromJson(configRaw));
    final state = AppState(config: config, prefs: prefs);
    await state._restoreRelaySyncCache();
    state._startRelaySyncLoop(immediate: config != null);
    return state;
  }

  Future<void> saveConfig(CoreConfig cfg) async {
    final previous = _config;
    _config = cfg;
    await SecretStore.saveConfigSecrets(cfg);
    ConfigStore.writeConfig(cfg.toSanitizedJson());
    if (previous != null &&
        (previous.username != cfg.username || previous.domain != cfg.domain)) {
      await RelaySyncStore.clear(previous);
      _clearRelaySyncState(notify: false);
    }
    await _restoreRelaySyncCache();
    _startRelaySyncLoop(immediate: true);
    notifyListeners();
  }

  Future<void> savePrefs(UiPrefs prefs) async {
    final prev = _prefs;
    _prefs = prefs;
    await SecretStore.savePrefsSecrets(prefs);
    await PrefsStore.write(UiPrefs.fromJson(prefs.toSanitizedJson()));
    _updateRelayDerivedState();
    notifyListeners();
    if ((prev.telemetryEnabled || prev.clientMonitoringEnabled) &&
        !(prefs.telemetryEnabled || prefs.clientMonitoringEnabled)) {
      await TelemetryService.clear();
    }
  }

  Future<void> clearConfig() async {
    final current = _config;
    _config = null;
    await SecretStore.clearConfigSecrets();
    ConfigStore.clear();
    PeerPresenceStore.instance.stop();
    _relaySyncTimer?.cancel();
    _relaySyncTimer = null;
    _relaySyncStreamRetry?.cancel();
    _relaySyncStreamRetry = null;
    _relaySyncStreamDebounce?.cancel();
    _relaySyncStreamDebounce = null;
    _relaySyncStreamSub?.cancel();
    _relaySyncStreamSub = null;
    _relaySyncStreamConnected = false;
    if (current != null) {
      await RelaySyncStore.clear(current);
    }
    _clearRelaySyncState(notify: false);
    notifyListeners();
  }

  Future<void> startCore() async {
    final cfg = _config;
    if (cfg == null) return;
    if (_coreHandle != null && !_externalCore) return;
    _lastError = null;
    try {
      var effective = cfg;

      final normalizedRelayWs =
          CoreConfig.normalizeRelayWs(effective.relayWs, publicBaseUrl: effective.publicBaseUrl);
      if (normalizedRelayWs != effective.relayWs.trim()) {
        effective = effective.copyWith(relayWs: normalizedRelayWs);
        await saveConfig(effective);
      }

      if (!(effective.relayWs.startsWith('ws://') || effective.relayWs.startsWith('wss://'))) {
        throw StateError('Relay WS must start with ws:// or wss:// (got: ${effective.relayWs})');
      }

      if (effective.relayToken.trim().length < 16) {
        final prev = effective.relayToken.trim();
        effective = effective.copyWith(
          relayToken: CoreConfig.randomToken(),
          previousRelayToken: prev.isEmpty ? effective.previousRelayToken : prev,
        );
        await saveConfig(effective);
      }
      if (effective.internalToken.trim().isEmpty) {
        effective = effective.copyWith(internalToken: CoreConfig.randomToken());
        await saveConfig(effective);
      }

      final api = CoreApi(config: effective);
      if (await api.checkHealth()) {
        _coreHandle = 0;
        _externalCore = true;
        _scheduleCloudBackup();
        _startRelaySyncLoop(immediate: true);
        notifyListeners();
        return;
      }

      final handle = Fedi3Core.instance.startJson(jsonEncode(effective.toCoreStartJson()));
      _coreHandle = handle;
      _externalCore = false;
      _scheduleCloudBackup();
      _startRelaySyncLoop(immediate: true);
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
    if (_externalCore) {
      _coreHandle = null;
      _externalCore = false;
      _cloudBackupTimer?.cancel();
      _cloudBackupTimer = null;
      _startRelaySyncLoop(immediate: false);
      notifyListeners();
      return;
    }
    try {
      Fedi3Core.instance.stop(handle);
      _coreHandle = null;
      _externalCore = false;
      _cloudBackupTimer?.cancel();
      _cloudBackupTimer = null;
      _startRelaySyncLoop(immediate: false);
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

  void _startRelaySyncLoop({required bool immediate}) {
    _relaySyncTimer?.cancel();
    _relaySyncStreamRetry?.cancel();
    final cfg = _config;
    _relaySyncStreamSub?.cancel();
    _relaySyncStreamSub = null;
    _relaySyncStreamConnected = false;
    if (cfg == null) return;
    _relaySyncTimer = Timer.periodic(const Duration(seconds: 90), (_) {
      unawaited(refreshRelaySync());
    });
    unawaited(_connectRelaySyncStream(cfg));
    if (immediate) {
      unawaited(refreshRelaySync(forceBootstrap: !_relaySyncReady));
    }
  }

  Future<void> _connectRelaySyncStream(CoreConfig cfg) async {
    try {
      final stream = await RelaySyncStreamClient(
        config: cfg,
        fallbackTokens: [
          cfg.previousRelayToken ?? '',
          _prefs.relayAdminToken,
        ],
      ).connect(
        sinceId: _relaySyncCursors.events > 0
            ? _relaySyncCursors.events
            : _relaySyncStreamLastEventId,
      );
      await _relaySyncStreamSub?.cancel();
      _relaySyncStreamSub = stream.listen(
        (ev) {
          _relaySyncStreamConnected = true;
          if (ev.eventId > _relaySyncStreamLastEventId) {
            _relaySyncStreamLastEventId = ev.eventId;
          }
          _relaySyncStreamDebounce?.cancel();
          _relaySyncStreamDebounce = Timer(
            const Duration(milliseconds: 220),
            () => unawaited(refreshRelaySync()),
          );
        },
        onDone: () => _scheduleRelaySyncStreamRetry(cfg),
        onError: (Object e, StackTrace st) {
          _relaySyncStreamConnected = false;
          _relaySyncError = e.toString();
          _scheduleRelaySyncStreamRetry(cfg);
        },
        cancelOnError: true,
      );
    } catch (e) {
      _relaySyncStreamConnected = false;
      _relaySyncError = e.toString();
      _scheduleRelaySyncStreamRetry(cfg);
    }
  }

  void _scheduleRelaySyncStreamRetry(CoreConfig cfg) {
    _relaySyncStreamRetry?.cancel();
    _relaySyncStreamRetry = Timer(const Duration(seconds: 2), () {
      if (!identical(_config, cfg)) return;
      unawaited(_connectRelaySyncStream(cfg));
    });
  }

  Future<void> refreshRelaySync({bool forceBootstrap = false}) async {
    final cfg = _config;
    if (cfg == null || _relaySyncBusy) return;
    _relaySyncBusy = true;
    _relaySyncError = null;
    notifyListeners();
    final api = RelaySyncApi(
      config: cfg,
      fallbackTokens: [
        cfg.previousRelayToken ?? '',
        _prefs.relayAdminToken,
      ],
    );
    try {
      final nowMs = DateTime.now().millisecondsSinceEpoch;
      final needsBootstrap = forceBootstrap ||
          !_relaySyncReady ||
          (_relayTimelineHome.isEmpty &&
              _relayNotifications.isEmpty &&
              _relayChatEntries.isEmpty);
      final canBootstrapNow =
          forceBootstrap || nowMs >= _relaySyncBootstrapRetryNotBeforeMs;
      if (needsBootstrap && canBootstrapNow) {
        final snapshot = await api.fetchBootstrap(
          eventLimit: 40,
          notificationLimit: 40,
          chatLimit: 40,
          timelineLimit: 40,
        );
        _applyRelaySnapshot(snapshot, persist: true);
        _relaySyncBootstrapFailCount = 0;
        _relaySyncBootstrapRetryNotBeforeMs = 0;
      } else {
        final eventsResp = await api.fetchEvents(sinceId: _relaySyncCursors.events);
        final notificationsResp =
            await api.fetchNotifications(sinceId: _relaySyncCursors.notifications);
        final chatResp = await api.fetchChat(sinceId: _relaySyncCursors.chat);
        final timelineResp =
            await api.fetchTimelineHome(since: _relaySyncCursors.timelineHome);

        final incomingEvents = _readMapList(eventsResp['items']);
        _relayEvents = _mergeByIntKey(
          _relayEvents,
          incomingEvents,
          key: 'event_id',
          maxItems: 500,
        );
        _relayNotifications = _mergeByIntKey(
          _relayNotifications,
          normalizeRelayNotifications(notificationsResp['items']),
          key: 'event_id',
          maxItems: 400,
        );
        _relayChatEntries = _mergeByIntKey(
          _relayChatEntries,
          normalizeRelayChatEntries(chatResp['items']),
          key: 'event_id',
          maxItems: 500,
        );
        _relayTimelineHome = _mergeTimelineEntries(
          _relayTimelineHome,
          normalizeRelayTimelineEntries(timelineResp['items']),
        );
        _applyRelayChatMutationsFromEvents(incomingEvents);
        _relaySyncCursors = SyncCursorState(
          events: _maxRelayId(_relayEvents, 'event_id'),
          notifications: _maxRelayId(_relayNotifications, 'event_id'),
          chat: _maxRelayId(_relayChatEntries, 'event_id'),
          timelineHome: _maxRelayId(_relayTimelineHome, 'cursor'),
        );
        _relaySyncReady = true;
        _relaySyncLastSuccessMs = DateTime.now().millisecondsSinceEpoch;
        _updateRelayDerivedState();
        await _persistRelaySnapshot();
      }
    } catch (e) {
      _relaySyncError = e.toString();
      final lower = _relaySyncError!.toLowerCase();
      if (lower.contains('sync bootstrap') &&
          (lower.contains('503') ||
              lower.contains('db busy') ||
              lower.contains('timeout'))) {
        _relaySyncBootstrapFailCount = (_relaySyncBootstrapFailCount + 1).clamp(1, 6);
        final backoffSecs = <int>[5, 10, 20, 30, 60, 120][_relaySyncBootstrapFailCount - 1];
        _relaySyncBootstrapRetryNotBeforeMs =
            DateTime.now().millisecondsSinceEpoch + backoffSecs * 1000;
      }
      TelemetryService.record('relay_sync_failed', _relaySyncError!);
    } finally {
      _relaySyncBusy = false;
      notifyListeners();
    }
  }

  Future<void> _restoreRelaySyncCache() async {
    final cfg = _config;
    if (cfg == null) {
      _clearRelaySyncState(notify: false);
      return;
    }
    final snapshot = await RelaySyncStore.read(cfg);
    if (snapshot == null) {
      _clearRelaySyncState(notify: false);
      return;
    }
    _applyRelaySnapshot(snapshot, persist: false, notify: false);
  }

  void _applyRelaySnapshot(
    RelaySyncBootstrapSnapshot snapshot, {
    required bool persist,
    bool notify = true,
  }) {
    _relaySyncCursors = snapshot.cursors;
    _relayEvents = List<Map<String, dynamic>>.from(snapshot.events);
    _relayNotifications = List<Map<String, dynamic>>.from(snapshot.notifications);
    _relayChatEntries = List<Map<String, dynamic>>.from(snapshot.chat);
    _relayTimelineHome = List<Map<String, dynamic>>.from(snapshot.timelineHome);
    _applyRelayChatMutationsFromEvents(snapshot.events);
    _relaySyncReady = true;
    _relaySyncError = null;
    _relaySyncLastSuccessMs =
        snapshot.generatedAtMs > 0 ? snapshot.generatedAtMs : DateTime.now().millisecondsSinceEpoch;
    if (_relaySyncCursors.events > _relaySyncStreamLastEventId) {
      _relaySyncStreamLastEventId = _relaySyncCursors.events;
    }
    _updateRelayDerivedState();
    if (persist && _config != null) {
      unawaited(_persistRelaySnapshot());
    }
    if (notify) {
      notifyListeners();
    }
  }

  Future<void> _persistRelaySnapshot() async {
    final cfg = _config;
    if (cfg == null) return;
    final snapshot = RelaySyncBootstrapSnapshot(
      generatedAtMs: _relaySyncLastSuccessMs,
      cursors: _relaySyncCursors,
      events: _relayEvents,
      notifications: _relayNotifications,
      chat: _relayChatEntries,
      timelineHome: _relayTimelineHome,
    );
    await RelaySyncStore.write(cfg, snapshot);
  }

  void _updateRelayDerivedState() {
    final cfg = _config;
    final actorBase = cfg == null
        ? ''
        : '${cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '')}/users/${cfg.username}';
    _relayChatThreads = deriveRelayChatThreads(
      _relayChatEntries,
      seenByThread: _prefs.chatThreadSeenMs,
      selfActor: actorBase,
    );
    _unreadNotifications = _relayNotifications
        .where((item) =>
            _readRelayInt(item['created_at_ms']) > _prefs.lastNotificationsSeenMs)
        .length;
    _unreadChats = _relayChatThreads.where((thread) {
      final ts = thread.lastMessageMs ?? thread.updatedAtMs;
      final seenMs = _prefs.chatThreadSeenMs[thread.threadId] ?? 0;
      return ts > seenMs;
    }).length;
  }

  void _applyRelayChatMutationsFromEvents(List<Map<String, dynamic>> events) {
    for (final event in events) {
      final kind = (event['event_kind'] as String?)?.trim() ?? '';
      if (kind != 'chat.message.deleted' && kind != 'chat.thread.deleted') {
        continue;
      }
      final payload = event['payload'];
      final payloadMap = payload is Map ? payload.cast<String, dynamic>() : const <String, dynamic>{};
      final threadId = (payloadMap['thread_id'] as String?)?.trim() ?? '';
      if (kind == 'chat.message.deleted') {
        final messageId = (payloadMap['message_id'] as String?)?.trim() ?? '';
        if (threadId.isEmpty || messageId.isEmpty) continue;
        _applyRelayChatMessageDeletedLocal(threadId: threadId, messageId: messageId);
      } else {
        if (threadId.isEmpty) continue;
        _applyRelayChatThreadDeletedLocal(threadId: threadId);
      }
    }
  }

  void _applyRelayChatMessageDeletedLocal({
    required String threadId,
    required String messageId,
  }) {
    _relayChatEntries = _relayChatEntries.where((entry) {
      final entryThread = (entry['thread_id'] as String?)?.trim() ?? '';
      final entryMessage = (entry['message_id'] as String?)?.trim() ?? '';
      if (entryThread != threadId) return true;
      return entryMessage != messageId;
    }).toList();
  }

  void _applyRelayChatThreadDeletedLocal({required String threadId}) {
    _relayChatEntries = _relayChatEntries.where((entry) {
      final entryThread = (entry['thread_id'] as String?)?.trim() ?? '';
      return entryThread != threadId;
    }).toList();
  }

  void _clearRelaySyncState({required bool notify}) {
    _relaySyncReady = false;
    _relaySyncBusy = false;
    _relaySyncError = null;
    _relaySyncLastSuccessMs = 0;
    _relaySyncBootstrapFailCount = 0;
    _relaySyncBootstrapRetryNotBeforeMs = 0;
    _relaySyncStreamLastEventId = 0;
    _relaySyncCursors = SyncCursorState.empty;
    _relayEvents = const [];
    _relayTimelineHome = const [];
    _relayNotifications = const [];
    _relayChatEntries = const [];
    _relayChatThreads = const [];
    _unreadNotifications = 0;
    _unreadChats = 0;
    if (notify) {
      notifyListeners();
    }
  }

  List<Map<String, dynamic>> _mergeByIntKey(
    List<Map<String, dynamic>> existing,
    List<Map<String, dynamic>> incoming, {
    required String key,
    required int maxItems,
  }) {
    final merged = <String, Map<String, dynamic>>{};
    for (final item in existing) {
      final id = _readRelayInt(item[key]).toString();
      if (id != '0') merged[id] = item;
    }
    for (final item in incoming) {
      final id = _readRelayInt(item[key]).toString();
      if (id != '0') merged[id] = item;
    }
    final items = merged.values.toList()
      ..sort((a, b) =>
          _readRelayInt(b[key]).compareTo(_readRelayInt(a[key])));
    if (items.length > maxItems) {
      items.removeRange(maxItems, items.length);
    }
    return items;
  }

  List<Map<String, dynamic>> _mergeTimelineEntries(
    List<Map<String, dynamic>> existing,
    List<Map<String, dynamic>> incoming,
  ) {
    final merged = <String, Map<String, dynamic>>{};
    for (final item in existing) {
      merged[_timelineIdentity(item)] = item;
    }
    for (final item in incoming) {
      merged[_timelineIdentity(item)] = item;
    }
    final items = merged.values.toList()
      ..sort((a, b) =>
          _readRelayInt(b['cursor'] ?? b['created_at_ms'])
              .compareTo(_readRelayInt(a['cursor'] ?? a['created_at_ms'])));
    return items.take(400).toList();
  }

  String _timelineIdentity(Map<String, dynamic> item) {
    final id = (item['id'] as String?)?.trim() ?? '';
    if (id.isNotEmpty) return id;
    final object = item['object'];
    if (object is Map) {
      final objectId = (object['id'] as String?)?.trim() ?? '';
      if (objectId.isNotEmpty) return objectId;
    }
    return 'cursor:${_readRelayInt(item['cursor'] ?? item['created_at_ms'])}';
  }

  int _maxRelayId(List<Map<String, dynamic>> items, String key) {
    var max = 0;
    for (final item in items) {
      final value = _readRelayInt(item[key]);
      if (value > max) max = value;
    }
    return max;
  }

  List<Map<String, dynamic>> _readMapList(dynamic raw) {
    if (raw is! List) return const [];
    return raw
        .whereType<Map>()
        .map((value) => value.cast<String, dynamic>())
        .toList();
  }

  int _readRelayInt(dynamic raw) {
    if (raw is num) return raw.toInt();
    if (raw is String) return int.tryParse(raw.trim()) ?? 0;
    return 0;
  }

  @override
  void dispose() {
    _cloudBackupTimer?.cancel();
    _relaySyncTimer?.cancel();
    _relaySyncStreamRetry?.cancel();
    _relaySyncStreamDebounce?.cancel();
    _relaySyncStreamSub?.cancel();
    super.dispose();
  }
}
