/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../model/note_models.dart';
import '../../services/core_event_stream.dart';
import '../../state/app_state.dart';
import '../widgets/inline_composer.dart';
import '../widgets/core_not_running_card.dart';
import '../widgets/network_error_card.dart';
import '../widgets/rss_ticker.dart';
import '../widgets/timeline_activity_card.dart';
import '../theme/ui_tokens.dart';

class TimelinesScreen extends StatefulWidget {
  const TimelinesScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<TimelinesScreen> createState() => _TimelinesScreenState();
}

class _TimelinesScreenState extends State<TimelinesScreen>
    with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 4, vsync: this);
  late final List<GlobalKey<_TimelineListState>> _listKeys =
      List.generate(4, (_) => GlobalKey<_TimelineListState>());
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  Timer? _syncStatusPoll;
  CoreConfig? _streamConfig;
  Map<String, dynamic>? _syncStatus;
  bool _syncStatusLoading = true;
  String? _syncStatusError;
  final ScrollController _columnsScroll = ScrollController();

  @override
  void initState() {
    super.initState();
    _startSyncStatusPolling();
  }

  @override
  void dispose() {
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
    _streamSub?.cancel();
    _syncStatusPoll?.cancel();
    _columnsScroll.dispose();
    _tabs.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final cfg = widget.appState.config!;
        final api = CoreApi(config: cfg);
        if (widget.appState.isRunning && _syncStatusPoll == null) {
          _startSyncStatusPolling();
        }
        _ensureStream(cfg);
        final isWide =
            MediaQuery.of(context).size.width >= UiTokens.desktopBreakpoint;
        final useColumns = isWide && widget.appState.prefs.desktopUseColumns;
        final showInlineComposer = !isWide;

        if (!widget.appState.isRunning) {
          return Scaffold(
            appBar: AppBar(
              title: Text(context.l10n.timelineTitle),
            ),
            body: CoreNotRunningCard(
              appState: widget.appState,
              hint: context.l10n.timelineRefreshHint,
              onStarted: () {
                if (!mounted) return;
                for (final k in _listKeys) {
                  k.currentState?.refreshFromStream();
                }
              },
            ),
          );
        }

        final syncReady = (_syncStatus?['ready'] == true);
        final syncBlocked = _syncStatusLoading || !syncReady;
        final syncStale = _isSyncStale(_syncStatus);

        final timelineBody = isWide
            ? (useColumns
                ? Scrollbar(
                    controller: _columnsScroll,
                    thumbVisibility: true,
                    child: SingleChildScrollView(
                      controller: _columnsScroll,
                      scrollDirection: Axis.horizontal,
                      padding: const EdgeInsets.symmetric(
                          horizontal: 12, vertical: 10),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          _TimelineList(
                            key: _listKeys[0],
                            appState: widget.appState,
                            api: api,
                            kind: 'unified',
                            headerTitle: context.l10n.timelineTabHome,
                            headerTooltip: context.l10n.timelineHomeTooltip,
                            constrainWidth: false,
                            showComposer: showInlineComposer,
                          ),
                          const SizedBox(width: 10),
                          _TimelineList(
                            key: _listKeys[1],
                            appState: widget.appState,
                            api: api,
                            kind: 'local',
                            headerTitle: context.l10n.timelineTabLocal,
                            headerTooltip: context.l10n.timelineLocalTooltip,
                            constrainWidth: false,
                            showComposer: showInlineComposer,
                          ),
                          const SizedBox(width: 10),
                          _TimelineList(
                            key: _listKeys[2],
                            appState: widget.appState,
                            api: api,
                            kind: 'social',
                            headerTitle: context.l10n.timelineTabSocial,
                            headerTooltip: context.l10n.timelineSocialTooltip,
                            constrainWidth: false,
                            showComposer: showInlineComposer,
                          ),
                          const SizedBox(width: 10),
                          _TimelineList(
                            key: _listKeys[3],
                            appState: widget.appState,
                            api: api,
                            kind: 'federated',
                            headerTitle: context.l10n.timelineTabFederated,
                            headerTooltip:
                                context.l10n.timelineFederatedTooltip,
                            constrainWidth: false,
                            showComposer: showInlineComposer,
                          ),
                        ],
                      ),
                    ),
                  )
                : TabBarView(
                    controller: _tabs,
                    children: [
                      _TimelineList(
                        key: _listKeys[0],
                        appState: widget.appState,
                        api: api,
                        kind: 'unified',
                        headerTitle: context.l10n.timelineTabHome,
                        headerTooltip: context.l10n.timelineHomeTooltip,
                        constrainWidth: true,
                        showComposer: showInlineComposer,
                      ),
                      _TimelineList(
                        key: _listKeys[1],
                        appState: widget.appState,
                        api: api,
                        kind: 'local',
                        headerTitle: context.l10n.timelineTabLocal,
                        headerTooltip: context.l10n.timelineLocalTooltip,
                        constrainWidth: true,
                        showComposer: showInlineComposer,
                      ),
                      _TimelineList(
                        key: _listKeys[2],
                        appState: widget.appState,
                        api: api,
                        kind: 'social',
                        headerTitle: context.l10n.timelineTabSocial,
                        headerTooltip: context.l10n.timelineSocialTooltip,
                        constrainWidth: true,
                        showComposer: showInlineComposer,
                      ),
                      _TimelineList(
                        key: _listKeys[3],
                        appState: widget.appState,
                        api: api,
                        kind: 'federated',
                        headerTitle: context.l10n.timelineTabFederated,
                        headerTooltip: context.l10n.timelineFederatedTooltip,
                        constrainWidth: true,
                        showComposer: showInlineComposer,
                      ),
                    ],
                  ))
            : TabBarView(
                controller: _tabs,
                children: [
                  _TimelineList(
                    key: _listKeys[0],
                    appState: widget.appState,
                    api: api,
                    kind: 'unified',
                    headerTitle: context.l10n.timelineTabHome,
                    headerTooltip: context.l10n.timelineHomeTooltip,
                    constrainWidth: true,
                    showComposer: showInlineComposer,
                  ),
                  _TimelineList(
                    key: _listKeys[1],
                    appState: widget.appState,
                    api: api,
                    kind: 'local',
                    headerTitle: context.l10n.timelineTabLocal,
                    headerTooltip: context.l10n.timelineLocalTooltip,
                    constrainWidth: true,
                    showComposer: showInlineComposer,
                  ),
                  _TimelineList(
                    key: _listKeys[2],
                    appState: widget.appState,
                    api: api,
                    kind: 'social',
                    headerTitle: context.l10n.timelineTabSocial,
                    headerTooltip: context.l10n.timelineSocialTooltip,
                    constrainWidth: true,
                    showComposer: showInlineComposer,
                  ),
                  _TimelineList(
                    key: _listKeys[3],
                    appState: widget.appState,
                    api: api,
                    kind: 'federated',
                    headerTitle: context.l10n.timelineTabFederated,
                    headerTooltip: context.l10n.timelineFederatedTooltip,
                    constrainWidth: true,
                    showComposer: showInlineComposer,
                  ),
                ],
              );

        return Scaffold(
          appBar: AppBar(
            title: InkWell(
              borderRadius: BorderRadius.circular(10),
              onTap: () {
                final idx = _tabs.index;
                if (idx >= 0 && idx < _listKeys.length) {
                  _listKeys[idx].currentState?.scrollToTop();
                }
              },
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
                child: Text(context.l10n.timelineTitle),
              ),
            ),
            bottom: isWide
                ? (useColumns
                    ? null
                    : TabBar(
                        controller: _tabs,
                        tabs: [
                          Tooltip(
                            message: context.l10n.timelineHomeTooltip,
                            child: Tab(text: context.l10n.timelineTabHome),
                          ),
                          Tooltip(
                            message: context.l10n.timelineLocalTooltip,
                            child: Tab(text: context.l10n.timelineTabLocal),
                          ),
                          Tooltip(
                            message: context.l10n.timelineSocialTooltip,
                            child: Tab(text: context.l10n.timelineTabSocial),
                          ),
                          Tooltip(
                            message: context.l10n.timelineFederatedTooltip,
                            child: Tab(text: context.l10n.timelineTabFederated),
                          ),
                        ],
                      ))
                : TabBar(
                    controller: _tabs,
                    tabs: [
                      Tooltip(
                        message: context.l10n.timelineHomeTooltip,
                        child: Tab(text: context.l10n.timelineTabHome),
                      ),
                      Tooltip(
                        message: context.l10n.timelineLocalTooltip,
                        child: Tab(text: context.l10n.timelineTabLocal),
                      ),
                      Tooltip(
                        message: context.l10n.timelineSocialTooltip,
                        child: Tab(text: context.l10n.timelineTabSocial),
                      ),
                      Tooltip(
                        message: context.l10n.timelineFederatedTooltip,
                        child: Tab(text: context.l10n.timelineTabFederated),
                      ),
                    ],
                  ),
            actions: [
              if (isWide)
                IconButton(
                  tooltip: useColumns
                      ? context.l10n.timelineLayoutTabs
                      : context.l10n.timelineLayoutColumns,
                  onPressed: () async {
                    final prefs = widget.appState.prefs;
                    await widget.appState.savePrefs(prefs.copyWith(
                        desktopUseColumns: !prefs.desktopUseColumns));
                  },
                  icon: Icon(useColumns
                      ? Icons.view_agenda_outlined
                      : Icons.view_week_outlined),
                ),
            ],
          ),
          body: Column(
            children: [
              const RssTicker(),
              if (syncBlocked || syncStale || _syncStatusError != null)
                _buildSyncInlineBanner(
                  context,
                  syncBlocked: syncBlocked,
                  syncStale: syncStale,
                ),
              Expanded(child: timelineBody),
            ],
          ),
        );
      },
    );
  }

  Widget _buildSyncInlineBanner(
    BuildContext context, {
    required bool syncBlocked,
    required bool syncStale,
  }) {
    final scheme = Theme.of(context).colorScheme;
    final hasCoreError =
        _syncStatusError != null && _syncStatusError!.trim().isNotEmpty;
    final relayError =
        ((_syncStatus?['last_error'] as String?)?.trim().isNotEmpty == true)
            ? (_syncStatus?['last_error'] as String).trim()
            : '';
    final title = syncBlocked
        ? 'Timeline in sincronizzazione'
        : (syncStale ? 'Sync timeline in ritardo' : 'Sync timeline');
    final subtitle = hasCoreError
        ? 'Core locale: ${_syncStatusError!.trim()}'
        : (relayError.isNotEmpty
            ? 'Relay/sync: $relayError'
            : _syncPhaseText(_syncStatus));
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(12, 10, 12, 0),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest.withAlpha(90),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: scheme.outlineVariant.withAlpha(110)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                syncStale ? Icons.warning_amber_rounded : Icons.sync,
                size: 16,
                color: syncStale ? scheme.error : scheme.primary,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(title,
                    style: const TextStyle(fontWeight: FontWeight.w700)),
              ),
              OutlinedButton.icon(
                onPressed: () => _pollSyncStatus(forceLegacySync: true),
                icon: const Icon(Icons.refresh, size: 16),
                label: Text(syncStale ? 'Recovery' : 'Aggiorna'),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(subtitle, style: const TextStyle(fontSize: 12)),
          const SizedBox(height: 6),
          Wrap(spacing: 8, runSpacing: 8, children: [
            if (_syncStatus != null) ..._buildStreamChips(_syncStatus!),
          ]),
        ],
      ),
    );
  }

  void _ensureStream(cfg) {
    final running = widget.appState.isRunning;
    final configChanged = !identical(_streamConfig, cfg);
    if (!running) {
      _streamRetry?.cancel();
      _streamSub?.cancel();
      _streamSub = null;
      _streamConfig = cfg;
      return;
    }
    if (configChanged) {
      _streamSub?.cancel();
      _streamSub = null;
    }
    if (_streamSub != null) {
      return;
    }
    _streamConfig = cfg;
    _streamRetry?.cancel();
    _streamSub = CoreEventStream(config: cfg).stream().listen((ev) {
      if (!mounted) return;
      if (ev.kind != 'timeline' && ev.kind != 'inbox' && ev.kind != 'outbox') {
        return;
      }
      final ty = (ev.activityType ?? '').toLowerCase();
      final forceRefresh = ty == 'update' ||
          ty == 'delete' ||
          ty == 'undo' ||
          ty == 'like' ||
          ty == 'emojireact' ||
          ty == 'announce' ||
          ty == 'create';
      _streamDebounce?.cancel();
      _streamDebounce = Timer(const Duration(milliseconds: 250), () {
        for (final k in _listKeys) {
          if (forceRefresh) {
            k.currentState?.refreshFromStream();
          } else {
            k.currentState?.pollNewFromStream();
          }
        }
      });
    }, onError: (_) => _scheduleStreamRetry(), onDone: _scheduleStreamRetry);
  }

  void _scheduleStreamRetry() {
    if (!mounted) return;
    _streamSub?.cancel();
    _streamSub = null;
    if (!widget.appState.isRunning) return;
    _streamRetry?.cancel();
    _streamRetry = Timer(const Duration(seconds: 2), () {
      if (!mounted) return;
      final cfg = widget.appState.config;
      if (cfg == null) return;
      _ensureStream(cfg);
    });
  }

  void _startSyncStatusPolling() {
    _syncStatusPoll?.cancel();
    _syncStatusPoll =
        Timer.periodic(const Duration(seconds: 3), (_) => _pollSyncStatus());
    _pollSyncStatus();
  }

  Future<void> _pollSyncStatus({bool forceLegacySync = false}) async {
    if (!mounted) return;
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    if (forceLegacySync) {
      try {
        await api.triggerLegacySync(pages: 12, itemsPerActor: 600);
      } catch (_) {
        // ignore; status poll below reports actual state
      }
    }
    try {
      final status = await api.fetchLegacySyncStatus();
      if (!mounted) return;
      setState(() {
        _syncStatus = status;
        _syncStatusLoading = false;
        _syncStatusError = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _syncStatusLoading = false;
        _syncStatusError = e.toString();
      });
    }
  }

  String _syncPhaseText(Map<String, dynamic>? status) {
    final phase =
        (status?['phase'] as String?)?.trim().toLowerCase() ?? 'unknown';
    switch (phase) {
      case 'bootstrap':
      case 'bootstrap_download':
        return 'Fase 1/3: bootstrap download';
      case 'delta':
      case 'delta_catchup':
        return 'Fase 3/3: delta catch-up';
      case 'apply':
        return 'Fase 2/3: apply baseline';
      case 'idle':
      case 'ready':
        return 'Baseline pronta';
      case 'error':
        return 'Errore sync: tentativo di recovery in corso';
      default:
        return 'Preparazione baseline timeline';
    }
  }

  bool _isSyncStale(Map<String, dynamic>? status) {
    if (status == null) return false;
    if (status['ready'] == true) return false;
    final streams = (status['streams'] is Map)
        ? (status['streams'] as Map).cast<String, dynamic>()
        : <String, dynamic>{};
    int latestOk = 0;
    for (final row in streams.values) {
      if (row is! Map) continue;
      final cast = row.cast<String, dynamic>();
      final ms = cast['last_ok_ms'];
      if (ms is num && ms.toInt() > latestOk) latestOk = ms.toInt();
    }
    final nowMs = DateTime.now().millisecondsSinceEpoch;
    if (latestOk <= 0) {
      return !_syncStatusLoading;
    }
    return nowMs - latestOk > const Duration(minutes: 2).inMilliseconds;
  }

  List<Widget> _buildStreamChips(Map<String, dynamic> status) {
    final streams = (status['streams'] is Map)
        ? (status['streams'] as Map).cast<String, dynamic>()
        : <String, dynamic>{};
    const order = ['home', 'social', 'local', 'federated'];
    return order.map((name) {
      final row = (streams[name] is Map)
          ? (streams[name] as Map).cast<String, dynamic>()
          : const <String, dynamic>{};
      final ready = row['ready'] == true;
      final streamError = (row['last_error'] as String?)?.trim() ?? '';
      final lag = row['lag_ms'];
      final lagLabel =
          (lag is num && lag >= 0) ? '${(lag / 1000).round()}s' : '-';
      final statusLabel =
          streamError.isNotEmpty ? 'ERR' : (ready ? 'READY' : 'WAIT');
      return Chip(
        label: Text('${name.toUpperCase()} $statusLabel - lag $lagLabel'),
      );
    }).toList();
  }
}

class _TimelineList extends StatefulWidget {
  const _TimelineList({
    super.key,
    required this.appState,
    required this.api,
    required this.kind,
    required this.headerTitle,
    required this.headerTooltip,
    required this.constrainWidth,
    required this.showComposer,
  });

  final AppState appState;
  final CoreApi api;
  final String kind;
  final String headerTitle;
  final String headerTooltip;
  final bool constrainWidth;
  final bool showComposer;

  @override
  State<_TimelineList> createState() => _TimelineListState();
}

class _TimelineListState extends State<_TimelineList>
    with AutomaticKeepAliveClientMixin {
  String? _cursor;
  bool _loading = false;
  bool _syncing = false;
  String? _error;
  final _items = <Map<String, dynamic>>[];
  final _knownIds = <String>{};
  final _knownObjectIds = <String>{};
  final _pending = <Map<String, dynamic>>[];
  final _scroll = ScrollController();
  Timer? _poll;
  Timer? _retry;
  bool _showScrollTop = false;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;

  @override
  bool get wantKeepAlive => true;

  void scrollToTop() {
    if (!_scroll.hasClients) return;
    _scroll.animateTo(
      0,
      duration: const Duration(milliseconds: 450),
      curve: Curves.easeOutCubic,
    );
    if (_pending.isNotEmpty) _applyPending();
  }

  void pollNewFromStream() {
    if (!mounted) return;
    if (!widget.appState.isRunning) return;
    _pollNew();
  }

  void refreshFromStream() {
    if (!mounted) return;
    if (!widget.appState.isRunning) return;
    _refresh();
  }

  @override
  void initState() {
    super.initState();
    _refresh();
    _poll = Timer.periodic(const Duration(seconds: 5), (_) => _pollNew());
    _scroll.addListener(_onScroll);
    _lastRunning = widget.appState.isRunning;
    _appStateListener = () {
      final running = widget.appState.isRunning;
      if (running && !_lastRunning) {
        if (mounted) _refresh();
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);
  }

  @override
  void dispose() {
    _poll?.cancel();
    _retry?.cancel();
    widget.appState.removeListener(_appStateListener);
    _scroll.removeListener(_onScroll);
    _scroll.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (!_scroll.hasClients) return;
    final show = _scroll.offset > 420;
    if (show != _showScrollTop) {
      setState(() => _showScrollTop = show);
    }
  }

  Future<void> _refresh() async {
    setState(() {
      _cursor = null;
      _items.clear();
      _pending.clear();
      _knownIds.clear();
      _knownObjectIds.clear();
    });
    await _loadMore();
  }

  Future<void> _forceSyncAndRefresh() async {
    if (_syncing) return;
    setState(() {
      _syncing = true;
      _error = null;
    });
    try {
      await widget.api.triggerLegacySync(
        pages: 10,
        itemsPerActor: 400,
        resetCheckpoints: _shouldAttemptRecoveryReset(),
      );
    } catch (e) {
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _syncing = false);
    }
    await _refresh();
  }

  bool _shouldAttemptRecoveryReset() {
    if (_items.isEmpty) {
      return true;
    }
    final newest = _activityTimestampMs(_items.first);
    if (newest <= 0) {
      return true;
    }
    final ageMs = DateTime.now().millisecondsSinceEpoch - newest;
    return ageMs > const Duration(hours: 6).inMilliseconds;
  }

  Future<void> _loadMore() async {
    if (_loading) return;
    if (!widget.appState.isRunning) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final resp = await widget.api.fetchTimeline(widget.kind, cursor: _cursor);
      final items = (resp['items'] as List<dynamic>? ?? [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .where((m) => !_isNoisyActivity(m))
          .where(_isRenderableTimelineActivity)
          .toList();
      setState(() {
        for (final it in items) {
          if (_mergeExisting(it)) continue;
          _registerKnown(it);
          _items.add(it);
        }
        _cursor = _readCursor(resp['next']);
        _sortActivities(_items);
      });
    } catch (e) {
      final msg = e.toString();
      setState(() => _error = msg);
      _scheduleRetryIfOffline(msg);
    } finally {
      setState(() => _loading = false);
    }
  }

  Future<void> _pollNew() async {
    if (!mounted) return;
    if (_loading) return;
    if (!widget.appState.isRunning) return;
    try {
      final resp = await widget.api.fetchTimeline(widget.kind, limit: 20);
      final items = (resp['items'] as List<dynamic>? ?? [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .where((m) => !_isNoisyActivity(m))
          .where(_isRenderableTimelineActivity)
          .toList();
      if (items.isEmpty) return;

      final fresh = <Map<String, dynamic>>[];
      for (final it in items) {
        if (_mergeExisting(it)) continue;
        final id = _activityId(it);
        final objectId = _activityObjectId(it);
        if (id.isEmpty && objectId == null) continue;
        _registerKnown(it);
        fresh.add(it);
      }
      if (fresh.isEmpty) return;

      final atTop = _scroll.hasClients ? _scroll.offset <= 80 : true;
      setState(() {
        _sortActivities(fresh);
        if (atTop && fresh.length <= 3) {
          _items.insertAll(0, fresh);
          _pending.clear();
          _sortActivities(_items);
        } else {
          _pending.addAll(fresh);
          _sortActivities(_pending);
        }
      });
    } catch (_) {
      // best-effort auto refresh: ignore transient failures
    }
  }

  void _applyPending() {
    if (_pending.isEmpty) return;
    setState(() {
      _items.insertAll(0, _pending);
      _pending.clear();
      _sortActivities(_items);
    });
    if (_scroll.hasClients) {
      _scroll.animateTo(
        0,
        duration: const Duration(milliseconds: 250),
        curve: Curves.easeOut,
      );
    }
  }

  void _scheduleRetryIfOffline(String msg) {
    if (!mounted) return;
    if (!widget.appState.isRunning) return;
    final lower = msg.toLowerCase();
    final shouldRetry = lower.contains('socketexception') ||
        lower.contains('connection refused') ||
        lower.contains('errno = 111') ||
        lower.contains('errno=111');
    if (!shouldRetry) return;
    if (_retry != null && _retry!.isActive) return;
    _retry = Timer(const Duration(seconds: 1), () {
      if (!mounted) return;
      if (!widget.appState.isRunning) return;
      _refresh();
    });
  }

  String _activityId(Map<String, dynamic> activity) {
    final id = activity['id'];
    if (id is String) return id.trim();
    return '';
  }

  String? _readCursor(dynamic value) {
    if (value == null) return null;
    if (value is num) {
      if (value <= 0) return null;
      return value.toInt().toString();
    }
    final s = value.toString().trim();
    if (s.isEmpty) return null;
    final asNum = int.tryParse(s);
    if (asNum != null && asNum <= 0) return null;
    return s;
  }

  int _activityTimestampMs(Map<String, dynamic> activity) {
    String readTime(Map<String, dynamic>? source) {
      if (source == null) return '';
      final published = (source['published'] as String?)?.trim() ?? '';
      if (published.isNotEmpty) return published;
      final updated = (source['updated'] as String?)?.trim() ?? '';
      if (updated.isNotEmpty) return updated;
      return '';
    }

    int readCreatedMs(Map<String, dynamic>? source) {
      if (source == null) return 0;
      final raw = source['created_at_ms'];
      if (raw is num) return raw.toInt();
      if (raw is String) return int.tryParse(raw.trim()) ?? 0;
      return 0;
    }

    var time = readTime(activity);
    if (time.isEmpty) {
      final obj = activity['object'];
      if (obj is Map) {
        final map = obj.cast<String, dynamic>();
        time = readTime(map);
        if (time.isEmpty && map['object'] is Map) {
          time = readTime((map['object'] as Map).cast<String, dynamic>());
        }
      }
    }
    final parsed = time.isEmpty ? null : DateTime.tryParse(time);
    if (parsed != null) {
      return parsed.millisecondsSinceEpoch;
    }
    var created = readCreatedMs(activity);
    if (created == 0) {
      final obj = activity['object'];
      if (obj is Map) {
        final map = obj.cast<String, dynamic>();
        created = readCreatedMs(map);
        if (created == 0 && map['object'] is Map) {
          created =
              readCreatedMs((map['object'] as Map).cast<String, dynamic>());
        }
      }
    }
    return created;
  }

  bool _isNoisyActivity(Map<String, dynamic> activity) {
    final type = (activity['type'] as String?)?.trim() ?? '';
    if (type != 'Announce') return false;
    final obj = activity['object'];
    return obj is String && obj.trim().isNotEmpty;
  }

  bool _isRenderableTimelineActivity(Map<String, dynamic> activity) {
    return TimelineItem.tryFromActivity(activity) != null;
  }

  String? _activityObjectId(Map<String, dynamic> activity) {
    final object = activity['object'];
    if (object is String) {
      final v = object.trim();
      return v.isEmpty ? null : v;
    }
    if (object is! Map) return null;
    final map = object.cast<String, dynamic>();
    final id = (map['id'] as String?)?.trim() ?? '';
    if (id.isNotEmpty) return id;
    final inner = map['object'];
    if (inner is Map) {
      final iid = (inner['id'] as String?)?.trim() ?? '';
      if (iid.isNotEmpty) return iid;
    }
    return null;
  }

  void _sortActivities(List<Map<String, dynamic>> items) {
    items.sort((a, b) {
      final ta = _activityTimestampMs(a);
      final tb = _activityTimestampMs(b);
      if (ta != tb) return tb.compareTo(ta);
      return _activityId(b).compareTo(_activityId(a));
    });
  }

  bool _mergeExisting(Map<String, dynamic> incoming) {
    final id = _activityId(incoming);
    final objectId = _activityObjectId(incoming);
    bool replaceIn(List<Map<String, dynamic>> target) {
      for (var i = 0; i < target.length; i++) {
        final current = target[i];
        final sameId = id.isNotEmpty && _activityId(current) == id;
        final sameObject =
            objectId != null && _activityObjectId(current) == objectId;
        if (!sameId && !sameObject) continue;
        if (!_sameActivityPayload(current, incoming)) {
          target[i] = incoming;
        }
        return true;
      }
      return false;
    }

    return replaceIn(_items) || replaceIn(_pending);
  }

  bool _sameActivityPayload(
    Map<String, dynamic> a,
    Map<String, dynamic> b,
  ) {
    final aa = a['object'];
    final bb = b['object'];
    String objectId(dynamic obj) {
      if (obj is String) return obj.trim();
      if (obj is Map) return (obj['id'] as String?)?.trim() ?? '';
      return '';
    }

    return _activityTimestampMs(a) == _activityTimestampMs(b) &&
        objectId(aa) == objectId(bb) &&
        (a['type']?.toString() ?? '') == (b['type']?.toString() ?? '');
  }

  void _registerKnown(Map<String, dynamic> activity) {
    final id = _activityId(activity);
    final objectId = _activityObjectId(activity);
    if (id.isNotEmpty) _knownIds.add(id);
    if (objectId != null) _knownObjectIds.add(objectId);
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    final filtered = _items;
    final timelineCache = <Map<String, dynamic>, TimelineItem?>{};
    final notesById = <String, TimelineItem>{};
    for (final activity in filtered) {
      final item = TimelineItem.tryFromActivity(activity);
      timelineCache[activity] = item;
      final noteId = item?.note.id.trim() ?? '';
      if (noteId.isNotEmpty) {
        notesById[noteId] = item!;
      }
    }
    final pendingCount = _pending.length;
    final panelColor = Theme.of(context).colorScheme.surfaceContainerLow;
    final panelBorder =
        Theme.of(context).colorScheme.outlineVariant.withAlpha(90);
    final list = RefreshIndicator(
      onRefresh: _forceSyncAndRefresh,
      child: Container(
        margin: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: panelColor,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: panelBorder),
        ),
        child: ClipRRect(
          borderRadius: BorderRadius.circular(16),
          child: ListView.builder(
            key: PageStorageKey('timeline-${widget.kind}'),
            controller: _scroll,
            padding: EdgeInsets.zero,
            cacheExtent: 1200,
            addAutomaticKeepAlives: false,
            itemCount: filtered.length + 3,
            itemBuilder: (context, index) {
              if (index == 0) {
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Container(
                      padding: const EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        border: Border(bottom: BorderSide(color: panelBorder)),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Row(
                            children: [
                              Tooltip(
                                message: widget.headerTooltip,
                                child: Text(
                                  widget.headerTitle,
                                  style: const TextStyle(
                                      fontWeight: FontWeight.w800),
                                ),
                              ),
                              if (pendingCount > 0) ...[
                                const SizedBox(width: 8),
                                InkWell(
                                  onTap: _applyPending,
                                  borderRadius: BorderRadius.circular(10),
                                  child: Container(
                                    padding: const EdgeInsets.symmetric(
                                        horizontal: 8, vertical: 4),
                                    decoration: BoxDecoration(
                                      color: Theme.of(context)
                                          .colorScheme
                                          .primary
                                          .withAlpha(30),
                                      borderRadius: BorderRadius.circular(10),
                                    ),
                                    child: Text(
                                      context.l10n
                                          .timelineNewPosts(pendingCount),
                                      style: TextStyle(
                                        color: Theme.of(context)
                                            .colorScheme
                                            .primary,
                                        fontSize: 11,
                                      ),
                                    ),
                                  ),
                                ),
                              ],
                              const Spacer(),
                              IconButton(
                                tooltip: context.l10n.timelineRefreshHint,
                                onPressed:
                                    widget.appState.isRunning && !_syncing
                                        ? _forceSyncAndRefresh
                                        : null,
                                icon: _syncing
                                    ? const SizedBox(
                                        width: 18,
                                        height: 18,
                                        child: CircularProgressIndicator(
                                            strokeWidth: 2),
                                      )
                                    : const Icon(Icons.refresh),
                              ),
                            ],
                          ),
                          const SizedBox(height: 8),
                        ],
                      ),
                    ),
                    if (widget.showComposer) ...[
                      InlineComposer(
                        appState: widget.appState,
                        api: widget.api,
                        onPosted: _refresh,
                      ),
                      const SizedBox(height: 10),
                    ],
                  ],
                );
              }
              if (index == 1) {
                if (pendingCount > 0) {
                  return Container(
                    padding: const EdgeInsets.fromLTRB(12, 12, 12, 12),
                    decoration: BoxDecoration(
                        border: Border(bottom: BorderSide(color: panelBorder))),
                    child: ListTile(
                      title: Text(context.l10n.timelineNewPosts(pendingCount)),
                      subtitle: Text(context.l10n.timelineShowNewPostsHint),
                      trailing: FilledButton(
                        onPressed: _applyPending,
                        child: Text(context.l10n.timelineShowNewPosts),
                      ),
                    ),
                  );
                }
                if (_error != null) {
                  return NetworkErrorCard(
                    message: _error,
                    onRetry: _refresh,
                    compact: true,
                  );
                }
                if (filtered.isEmpty && !_loading) {
                  return Padding(
                    padding: const EdgeInsets.symmetric(vertical: 24),
                    child: Center(child: Text(context.l10n.listNoItems)),
                  );
                }
                if (filtered.isEmpty && _loading) {
                  return const Column(
                    children: [
                      _TimelineSkeletonCard(),
                      SizedBox(height: 12),
                      _TimelineSkeletonCard(),
                      SizedBox(height: 12),
                      _TimelineSkeletonCard(),
                    ],
                  );
                }
                return const SizedBox.shrink();
              }
              if (index == filtered.length + 2) {
                return Padding(
                  padding: const EdgeInsets.symmetric(vertical: 12),
                  child: Center(
                    child: _loading
                        ? const CircularProgressIndicator()
                        : OutlinedButton(
                            onPressed: _cursor == null ? null : _loadMore,
                            child: Text(_cursor == null
                                ? context.l10n.listEnd
                                : context.l10n.listLoadMore),
                          ),
                  ),
                );
              }
              final item = filtered[index - 2];
              final isLast = index == filtered.length + 1;
              final timelineItem = timelineCache[item];
              final replyDepth = _replyDepth(timelineItem, notesById);
              final keyId = _activityId(item);
              final keyObjectId = _activityObjectId(item) ?? '';
              final stableKey = keyId.isNotEmpty
                  ? keyId
                  : (keyObjectId.isNotEmpty
                      ? keyObjectId
                      : '${widget.kind}-${timelineItem?.note.id ?? index}');
              final card = TimelineActivityCard(
                key: ValueKey(stableKey),
                appState: widget.appState,
                activity: item,
              );
              final nestedCard = replyDepth > 0
                  ? _NestedReplyLane(
                      depth: replyDepth,
                      child: card,
                    )
                  : card;
              return Container(
                decoration: BoxDecoration(
                  border: isLast
                      ? null
                      : Border(bottom: BorderSide(color: panelBorder)),
                ),
                padding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                child: nestedCard,
              );
            },
          ),
        ),
      ),
    );

    final fab = AnimatedScale(
      scale: _showScrollTop ? 1 : 0,
      duration: const Duration(milliseconds: 150),
      child: FloatingActionButton.small(
        heroTag: 'toTop-${widget.kind}',
        onPressed: _showScrollTop ? scrollToTop : null,
        child: const Icon(Icons.keyboard_arrow_up),
      ),
    );

    if (!widget.constrainWidth) {
      return SizedBox(
        width: UiTokens.timelineColumnWidth,
        child: Stack(
          children: [
            list,
            if (pendingCount > 0 && _scroll.hasClients && _scroll.offset > 120)
              Positioned(
                left: 12,
                right: 12,
                top: 12,
                child: Card(
                  child: ListTile(
                    title: Text(context.l10n.timelineNewPosts(pendingCount)),
                    trailing: FilledButton(
                      onPressed: _applyPending,
                      child: Text(context.l10n.timelineShowNewPosts),
                    ),
                  ),
                ),
              ),
            Positioned(
              right: 12,
              bottom: 12,
              child: fab,
            ),
          ],
        ),
      );
    }

    return Align(
      alignment: Alignment.topCenter,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: UiTokens.contentMaxWidth),
        child: Stack(
          children: [
            list,
            if (pendingCount > 0 && _scroll.hasClients && _scroll.offset > 120)
              Positioned(
                left: 12,
                right: 12,
                top: 12,
                child: Card(
                  child: ListTile(
                    title: Text(context.l10n.timelineNewPosts(pendingCount)),
                    trailing: FilledButton(
                      onPressed: _applyPending,
                      child: Text(context.l10n.timelineShowNewPosts),
                    ),
                  ),
                ),
              ),
            Positioned(
              right: 12,
              bottom: 12,
              child: fab,
            ),
          ],
        ),
      ),
    );
  }

  int _replyDepth(TimelineItem? item, Map<String, TimelineItem> notesById) {
    if (item == null) return 0;
    var parentId = item.note.inReplyTo.trim();
    if (parentId.isEmpty) return 0;
    var depth = 0;
    final seen = <String>{};
    while (parentId.isNotEmpty && depth < 3) {
      if (!seen.add(parentId)) break;
      final parent = notesById[parentId];
      if (parent == null) break;
      depth += 1;
      parentId = parent.note.inReplyTo.trim();
    }
    return depth;
  }
}

class _NestedReplyLane extends StatelessWidget {
  const _NestedReplyLane({required this.depth, required this.child});

  final int depth;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final laneColor = Theme.of(context).colorScheme.primary.withAlpha(85);
    final clampedDepth = depth.clamp(1, 3);
    return Padding(
      padding: EdgeInsets.only(left: 12.0 * clampedDepth),
      child: Stack(
        children: [
          Positioned.fill(
            child: Align(
              alignment: Alignment.centerLeft,
              child: Container(
                width: 2,
                decoration: BoxDecoration(
                  color: laneColor,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.only(left: 8),
            child: child,
          ),
        ],
      ),
    );
  }
}

class _TimelineSkeletonCard extends StatelessWidget {
  const _TimelineSkeletonCard();

  @override
  Widget build(BuildContext context) {
    final base =
        Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                    width: 42,
                    height: 42,
                    decoration:
                        BoxDecoration(color: base, shape: BoxShape.circle)),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Container(height: 12, width: 160, color: base),
                      const SizedBox(height: 8),
                      Container(height: 10, width: 110, color: base),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Container(height: 12, width: double.infinity, color: base),
            const SizedBox(height: 8),
            Container(height: 12, width: 240, color: base),
            const SizedBox(height: 8),
            Container(
                height: 180,
                width: double.infinity,
                decoration: BoxDecoration(
                    color: base, borderRadius: BorderRadius.circular(10))),
          ],
        ),
      ),
    );
  }
}
