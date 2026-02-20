/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/core_event_stream.dart';
import '../../state/app_state.dart';
import '../widgets/inline_composer.dart';
import '../widgets/network_error_card.dart';
import '../widgets/timeline_activity_card.dart';
import '../theme/ui_tokens.dart';

class TimelinesScreen extends StatefulWidget {
  const TimelinesScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<TimelinesScreen> createState() => _TimelinesScreenState();
}

class _TimelinesScreenState extends State<TimelinesScreen> with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 4, vsync: this);
  late final List<GlobalKey<_TimelineListState>> _listKeys =
      List.generate(4, (_) => GlobalKey<_TimelineListState>());
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  CoreConfig? _streamConfig;
  final ScrollController _columnsScroll = ScrollController();

  @override
  void dispose() {
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
    _streamSub?.cancel();
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
        _ensureStream(cfg);
        final isWide = MediaQuery.of(context).size.width >= UiTokens.desktopBreakpoint;
        final useColumns = isWide && widget.appState.prefs.desktopUseColumns;
        final showInlineComposer = !isWide;

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
              IconButton(
                tooltip: widget.appState.isRunning ? context.l10n.coreStop : context.l10n.coreStart,
                onPressed: () async {
                  if (widget.appState.isRunning) {
                    await widget.appState.stopCore();
                  } else {
                    await widget.appState.startCore();
                  }
                },
                icon: Icon(
                  widget.appState.isRunning ? Icons.stop_circle_outlined : Icons.play_circle_outline,
                ),
              ),
              if (isWide)
                IconButton(
                  tooltip: useColumns ? context.l10n.timelineLayoutTabs : context.l10n.timelineLayoutColumns,
                  onPressed: () async {
                    final prefs = widget.appState.prefs;
                    await widget.appState.savePrefs(prefs.copyWith(desktopUseColumns: !prefs.desktopUseColumns));
                  },
                  icon: Icon(useColumns ? Icons.view_agenda_outlined : Icons.view_week_outlined),
                ),
            ],
          ),
          body: isWide
              ? (useColumns
                  ? Scrollbar(
                  controller: _columnsScroll,
                  thumbVisibility: true,
                  child: SingleChildScrollView(
                    controller: _columnsScroll,
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
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
                          kind: 'home',
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
                          kind: 'dht',
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
                          headerTooltip: context.l10n.timelineFederatedTooltip,
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
                          kind: 'home',
                          headerTitle: context.l10n.timelineTabLocal,
                          headerTooltip: context.l10n.timelineLocalTooltip,
                          constrainWidth: true,
                          showComposer: showInlineComposer,
                        ),
                        _TimelineList(
                          key: _listKeys[2],
                          appState: widget.appState,
                          api: api,
                          kind: 'dht',
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
                      kind: 'home',
                      headerTitle: context.l10n.timelineTabLocal,
                      headerTooltip: context.l10n.timelineLocalTooltip,
                      constrainWidth: true,
                      showComposer: showInlineComposer,
                    ),
                    _TimelineList(
                      key: _listKeys[2],
                      appState: widget.appState,
                      api: api,
                      kind: 'dht',
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
                ),
        );
      },
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
    if (_streamSub != null) return;
    _streamConfig = cfg;
    _streamRetry?.cancel();
    _streamSub = CoreEventStream(config: cfg).stream().listen((ev) {
      if (!mounted) return;
      if (ev.kind != 'timeline' && ev.kind != 'inbox' && ev.kind != 'outbox') return;
      final ty = (ev.activityType ?? '').toLowerCase();
      final forceRefresh = ty == 'update' || ty == 'delete' || ty == 'undo';
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

class _TimelineListState extends State<_TimelineList> with AutomaticKeepAliveClientMixin {
  String? _cursor;
  bool _loading = false;
  bool _syncing = false;
  String? _error;
  final _items = <Map<String, dynamic>>[];
  final _knownIds = <String>{};
  final _pending = <Map<String, dynamic>>[];
  final _scroll = ScrollController();
  Timer? _poll;
  Timer? _retry;
  bool _showScrollTop = false;
  bool _filterMedia = false;
  bool _filterReplies = false;
  bool _filterBoosts = false;
  bool _filterMentions = false;
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
    _poll = Timer.periodic(const Duration(seconds: 8), (_) => _pollNew());
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
      await widget.api.triggerLegacySync(pages: 10, itemsPerActor: 400);
    } catch (e) {
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _syncing = false);
    }
    await _refresh();
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
          .where(_isTimelineItem)
          .toList();
      setState(() {
        for (final it in items) {
          final id = _activityId(it);
          if (id.isNotEmpty && _knownIds.contains(id)) continue;
          if (id.isNotEmpty) _knownIds.add(id);
          _items.add(it);
        }
        _cursor = (resp['next'] as String?)?.trim();
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
          .where(_isTimelineItem)
          .toList();
      if (items.isEmpty) return;

      final fresh = <Map<String, dynamic>>[];
      for (final it in items) {
        final id = _activityId(it);
        if (id.isEmpty) continue;
        if (_knownIds.contains(id)) continue;
        _knownIds.add(id);
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

  int _activityTimestampMs(Map<String, dynamic> activity) {
    String readTime(Map<String, dynamic>? source) {
      if (source == null) return '';
      final published = (source['published'] as String?)?.trim() ?? '';
      if (published.isNotEmpty) return published;
      final updated = (source['updated'] as String?)?.trim() ?? '';
      if (updated.isNotEmpty) return updated;
      return '';
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
    return parsed?.millisecondsSinceEpoch ?? 0;
  }

  void _sortActivities(List<Map<String, dynamic>> items) {
    items.sort((a, b) {
      final ta = _activityTimestampMs(a);
      final tb = _activityTimestampMs(b);
      if (ta != tb) return tb.compareTo(ta);
      return _activityId(b).compareTo(_activityId(a));
    });
  }

  bool _isTimelineItem(Map<String, dynamic> activity) {
    final type = (activity['type'] as String?)?.trim() ?? '';
    if (type != 'Create' && type != 'Announce') return false;

    final obj = activity['object'];
    if (obj is Map) {
      final m = obj.cast<String, dynamic>();
      if (type == 'Create') {
        final inner = m['object'];
        if (inner is Map) {
          final n = inner.cast<String, dynamic>();
          return (n['type'] as String?) == 'Note';
        }
        return (m['type'] as String?) == 'Note';
      }
      if (type == 'Announce') {
        if ((m['type'] as String?) == 'Note') return true;
        final inner = m['object'];
        if (inner is Map) return (inner['type'] as String?) == 'Note';
        if (inner is String) return inner.trim().isNotEmpty;
      }
    }
    if (obj is String) return obj.trim().isNotEmpty;
    return false;
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    final filtered = _items.where(_matchesFilters).toList(growable: false);
    final pendingCount = _pending.where(_matchesFilters).length;
    final panelColor = Theme.of(context).colorScheme.surfaceContainerLow;
    final panelBorder = Theme.of(context).colorScheme.outlineVariant.withAlpha(90);
    final list = RefreshIndicator(
      onRefresh: _refresh,
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
                                  style: const TextStyle(fontWeight: FontWeight.w800),
                                ),
                              ),
                              const Spacer(),
                              IconButton(
                                tooltip: context.l10n.timelineFilters,
                                onPressed: () => _showFilters(context),
                                icon: const Icon(Icons.filter_alt_outlined),
                              ),
                              IconButton(
                                tooltip: context.l10n.timelineRefreshHint,
                                onPressed: widget.appState.isRunning && !_syncing ? _forceSyncAndRefresh : null,
                                icon: _syncing
                                    ? const SizedBox(
                                        width: 18,
                                        height: 18,
                                        child: CircularProgressIndicator(strokeWidth: 2),
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
                    decoration: BoxDecoration(border: Border(bottom: BorderSide(color: panelBorder))),
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
                            child: Text(_cursor == null ? context.l10n.listEnd : context.l10n.listLoadMore),
                          ),
                  ),
                );
              }
              final item = filtered[index - 2];
              final isLast = index == filtered.length + 1;
              return Container(
                decoration: BoxDecoration(
                  border: isLast ? null : Border(bottom: BorderSide(color: panelBorder)),
                ),
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                child: TimelineActivityCard(
                  key: ValueKey(_activityId(item).isNotEmpty ? _activityId(item) : index),
                  appState: widget.appState,
                  activity: item,
                ),
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

  bool _matchesFilters(Map<String, dynamic> activity) {
    if (!_filterMedia && !_filterReplies && !_filterBoosts && !_filterMentions) return true;
    final type = (activity['type'] as String?)?.trim() ?? '';
    final obj = activity['object'];
    Map<String, dynamic>? noteMap;
    if (obj is Map) {
      final m = obj.cast<String, dynamic>();
      if (type == 'Create') {
        final inner = m['object'];
        if (inner is Map) noteMap = inner.cast<String, dynamic>();
        if (noteMap == null && (m['type'] as String?) == 'Note') noteMap = m;
      } else if (type == 'Announce') {
        if ((m['type'] as String?) == 'Note') {
          noteMap = m;
        } else if (m['object'] is Map) {
          noteMap = (m['object'] as Map).cast<String, dynamic>();
        }
      }
    }
    final attachments = noteMap?['attachment'];
    final hasMedia = attachments is List && attachments.isNotEmpty;
    final inReplyTo = (noteMap?['inReplyTo'] as String?)?.trim() ?? '';
    final hasMentions = (noteMap?['tag'] is List)
        ? (noteMap?['tag'] as List).whereType<Map>().any((m) => m['type'] == 'Mention')
        : false;
    final isBoost = type == 'Announce';

    if (_filterMedia && !hasMedia) return false;
    if (_filterReplies && inReplyTo.isEmpty) return false;
    if (_filterBoosts && !isBoost) return false;
    if (_filterMentions && !hasMentions) return false;
    return true;
  }

  void _showFilters(BuildContext context) {
    showModalBottomSheet(
      context: context,
      builder: (context) {
        return SafeArea(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        context.l10n.timelineFilters,
                        style: const TextStyle(fontWeight: FontWeight.w800),
                      ),
                    ),
                    IconButton(
                      tooltip: context.l10n.close,
                      onPressed: () => Navigator.of(context).pop(),
                      icon: const Icon(Icons.close),
                    ),
                  ],
                ),
                SwitchListTile(
                  title: Text(context.l10n.timelineFilterMedia),
                  value: _filterMedia,
                  onChanged: (v) => setState(() => _filterMedia = v),
                ),
                SwitchListTile(
                  title: Text(context.l10n.timelineFilterReply),
                  value: _filterReplies,
                  onChanged: (v) => setState(() => _filterReplies = v),
                ),
                SwitchListTile(
                  title: Text(context.l10n.timelineFilterBoost),
                  value: _filterBoosts,
                  onChanged: (v) => setState(() => _filterBoosts = v),
                ),
                SwitchListTile(
                  title: Text(context.l10n.timelineFilterMention),
                  value: _filterMentions,
                  onChanged: (v) => setState(() => _filterMentions = v),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _TimelineSkeletonCard extends StatelessWidget {
  const _TimelineSkeletonCard();

  @override
  Widget build(BuildContext context) {
    final base = Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(width: 42, height: 42, decoration: BoxDecoration(color: base, shape: BoxShape.circle)),
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
            Container(height: 180, width: double.infinity, decoration: BoxDecoration(color: base, borderRadius: BorderRadius.circular(10))),
          ],
        ),
      ),
    );
  }
}
