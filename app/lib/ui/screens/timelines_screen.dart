import '../../services/app_activity.dart';
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:material_ui/material_ui.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/actor_repository.dart';
import '../../services/core_event_stream.dart';
import '../../state/app_state.dart';
import 'compose_screen.dart';
import 'search_screen.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
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

  Widget _timelineTabs(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final labelStyle = Theme.of(context).textTheme.labelLarge;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
      child: TabBar(
        controller: _tabs,
        isScrollable: MediaQuery.sizeOf(context).width < 720,
        tabAlignment:
            MediaQuery.sizeOf(context).width < 720 ? TabAlignment.start : null,
        dividerColor: Colors.transparent,
        indicatorSize: TabBarIndicatorSize.tab,
        indicatorPadding:
            const EdgeInsets.symmetric(horizontal: 4, vertical: 5),
        indicator: BoxDecoration(
          borderRadius: BorderRadius.circular(UiTokens.radiusPill),
          color: scheme.primary.withAlpha(24),
          border: Border.all(color: scheme.primary.withAlpha(90)),
        ),
        labelColor: scheme.onSurface,
        unselectedLabelColor: scheme.onSurfaceVariant,
        labelStyle: labelStyle?.copyWith(fontWeight: FontWeight.w800),
        unselectedLabelStyle: Theme.of(context).textTheme.labelMedium,
        tabs: [
          Tooltip(
            message: context.l10n.timelineHomeTooltip,
            child: Tab(text: context.l10n.timelineTabHome),
          ),
          Tooltip(
            message: context.l10n.timelineLocalTooltip,
            child: Tab(text: context.l10n.timelineTabFollowing),
          ),
          Tooltip(
            message: context.l10n.timelineSocialTooltip,
            child: Tab(text: context.l10n.timelineTabDiscover),
          ),
          Tooltip(
            message: context.l10n.timelineFederatedTooltip,
            child: Tab(text: context.l10n.timelineTabFederated),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final cfg = widget.appState.config!;
        final api = CoreApi(config: cfg);
        _ensureStream(cfg);
        final isWide =
            MediaQuery.of(context).size.width >= UiTokens.desktopBreakpoint;
        final useColumns = isWide && widget.appState.prefs.desktopUseColumns;

        return Scaffold(
          floatingActionButton: isWide
              ? null
              : FloatingActionButton(
                  tooltip: context.l10n.composeTitle,
                  onPressed: () {
                    Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) =>
                            ComposeScreen(appState: widget.appState),
                      ),
                    );
                  },
                  child: const Icon(Icons.edit_rounded),
                ),
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
                    : PreferredSize(
                        preferredSize: Size.fromHeight(
                            48 * MediaQuery.textScalerOf(context).scale(1) + 4),
                        child: _timelineTabs(context)))
                : PreferredSize(
                    preferredSize: Size.fromHeight(
                        48 * MediaQuery.textScalerOf(context).scale(1) + 4),
                    child: _timelineTabs(context)),
            actions: [
              IconButton(
                tooltip: context.l10n.searchTitle,
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) => SearchScreen(appState: widget.appState),
                  ),
                ),
                icon: const Icon(Icons.search_rounded),
              ),
              IconButton(
                tooltip: widget.appState.isRunning
                    ? context.l10n.coreStop
                    : context.l10n.coreStart,
                onPressed: () async {
                  if (widget.appState.isRunning) {
                    await widget.appState.stopCore();
                  } else {
                    await widget.appState.startCore();
                  }
                },
                icon: Icon(
                  widget.appState.isRunning
                      ? Icons.stop_circle_outlined
                      : Icons.play_circle_outline,
                ),
              ),
              if (isWide)
                IconButton(
                  tooltip: useColumns
                      ? context.l10n.timelineLayoutTabs
                      : context.l10n.timelineLayoutColumns,
                  onPressed: () async {
                    await widget.appState.updatePrefs((current) =>
                        current.copyWith(
                            desktopUseColumns: !current.desktopUseColumns));
                  },
                  icon: Icon(useColumns
                      ? Icons.view_agenda_outlined
                      : Icons.view_week_outlined),
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
                        padding: const EdgeInsets.symmetric(
                            horizontal: 20, vertical: 16),
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
                            ),
                            const SizedBox(width: 16),
                            _TimelineList(
                              key: _listKeys[1],
                              appState: widget.appState,
                              api: api,
                              kind: 'home',
                              headerTitle: context.l10n.timelineTabFollowing,
                              headerTooltip: context.l10n.timelineLocalTooltip,
                              constrainWidth: false,
                            ),
                            const SizedBox(width: 16),
                            _TimelineList(
                              key: _listKeys[2],
                              appState: widget.appState,
                              api: api,
                              kind: 'dht',
                              headerTitle: context.l10n.timelineTabDiscover,
                              headerTooltip: context.l10n.timelineSocialTooltip,
                              constrainWidth: false,
                              header: _ExploreHeader(appState: widget.appState),
                            ),
                            const SizedBox(width: 16),
                            _TimelineList(
                              key: _listKeys[3],
                              appState: widget.appState,
                              api: api,
                              kind: 'federated',
                              headerTitle: context.l10n.timelineTabFederated,
                              headerTooltip:
                                  context.l10n.timelineFederatedTooltip,
                              constrainWidth: false,
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
                        ),
                        _TimelineList(
                          key: _listKeys[1],
                          appState: widget.appState,
                          api: api,
                          kind: 'home',
                          headerTitle: context.l10n.timelineTabFollowing,
                          headerTooltip: context.l10n.timelineLocalTooltip,
                          constrainWidth: true,
                        ),
                        _TimelineList(
                          key: _listKeys[2],
                          appState: widget.appState,
                          api: api,
                          kind: 'dht',
                          headerTitle: context.l10n.timelineTabDiscover,
                          headerTooltip: context.l10n.timelineSocialTooltip,
                          constrainWidth: true,
                          header: _ExploreHeader(appState: widget.appState),
                        ),
                        _TimelineList(
                          key: _listKeys[3],
                          appState: widget.appState,
                          api: api,
                          kind: 'federated',
                          headerTitle: context.l10n.timelineTabFederated,
                          headerTooltip: context.l10n.timelineFederatedTooltip,
                          constrainWidth: true,
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
                    ),
                    _TimelineList(
                      key: _listKeys[1],
                      appState: widget.appState,
                      api: api,
                      kind: 'home',
                      headerTitle: context.l10n.timelineTabFollowing,
                      headerTooltip: context.l10n.timelineLocalTooltip,
                      constrainWidth: true,
                    ),
                    _TimelineList(
                      key: _listKeys[2],
                      appState: widget.appState,
                      api: api,
                      kind: 'dht',
                      headerTitle: context.l10n.timelineTabDiscover,
                      headerTooltip: context.l10n.timelineSocialTooltip,
                      constrainWidth: true,
                    ),
                    _TimelineList(
                      key: _listKeys[3],
                      appState: widget.appState,
                      api: api,
                      kind: 'federated',
                      headerTitle: context.l10n.timelineTabFederated,
                      headerTooltip: context.l10n.timelineFederatedTooltip,
                      constrainWidth: true,
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
      if (ev.kind != 'timeline' && ev.kind != 'inbox' && ev.kind != 'outbox') {
        return;
      }
      _streamDebounce?.cancel();
      _streamDebounce = Timer(const Duration(milliseconds: 250), () {
        for (final k in _listKeys) {
          k.currentState?.pollNewFromStream();
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

class _ExploreHeader extends StatefulWidget {
  const _ExploreHeader({required this.appState});

  final AppState appState;

  @override
  State<_ExploreHeader> createState() => _ExploreHeaderState();
}

class _ExploreHeaderState extends State<_ExploreHeader> {
  List<Map<String, dynamic>> _trending = const [];
  List<ActorProfile> _suggested = const [];
  final Set<String> _followed = <String>{};
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final cfg = widget.appState.config;
    if (cfg == null || !widget.appState.isRunning) {
      if (mounted) setState(() => _loading = false);
      return;
    }
    final api = CoreApi(config: cfg);
    try {
      final results = await Future.wait([
        api.fetchTrendingHashtags(limit: 8),
        api.fetchSuggestedActors(limit: 6),
      ]);
      final tags = <Map<String, dynamic>>[];
      final rawTags = results[0]['items'];
      if (rawTags is List) {
        for (final it in rawTags) {
          if (it is Map) tags.add(it.cast<String, dynamic>());
        }
      }
      final actors = <ActorProfile>[];
      final rawActors = results[1]['items'];
      if (rawActors is List) {
        for (final it in rawActors) {
          if (it is! String) continue;
          final profile = await ActorRepository.instance.getActor(it);
          if (profile != null) actors.add(profile);
        }
      }
      if (!mounted) return;
      setState(() {
        _trending = tags;
        _suggested = actors;
        _loading = false;
      });
    } catch (_) {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _follow(ActorProfile profile) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    try {
      await api.follow(profile.id);
      if (!mounted) return;
      setState(() => _followed.add(profile.id));
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.notificationsFollowedBack)),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content:
              Text(context.l10n.notificationsFollowBackFailed(e.toString())),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Padding(
        padding: EdgeInsets.only(bottom: 12),
        child: Center(
          child: SizedBox(
            width: 18,
            height: 18,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
      );
    }
    if (_trending.isEmpty && _suggested.isEmpty) {
      return const SizedBox.shrink();
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (_trending.isNotEmpty)
            Card(
              child: Padding(
                padding: const EdgeInsets.all(UiTokens.padCard),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(Icons.trending_up_rounded,
                            size: 18,
                            color: Theme.of(context).colorScheme.primary),
                        const SizedBox(width: 8),
                        Text(context.l10n.trendingTitle,
                            style:
                                const TextStyle(fontWeight: FontWeight.w800)),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Text(
                      context.l10n.trendingHint,
                      style: TextStyle(
                        fontSize: 11.5,
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                      ),
                    ),
                    const SizedBox(height: 10),
                    Wrap(
                      spacing: 6,
                      runSpacing: 6,
                      children: [
                        for (final tag in _trending.take(8))
                          Builder(builder: (context) {
                            final name = (tag['name']?.toString() ?? '')
                                .trim()
                                .replaceFirst(RegExp(r'^#+'), '');
                            if (name.isEmpty) return const SizedBox.shrink();
                            return ActionChip(
                              label: Text('#$name',
                                  style: const TextStyle(fontSize: 12)),
                              onPressed: () => Navigator.of(context).push(
                                MaterialPageRoute<void>(
                                  builder: (_) => SearchScreen(
                                    appState: widget.appState,
                                    initialQuery: '#$name',
                                    initialTab: SearchTab.hashtags,
                                  ),
                                ),
                              ),
                            );
                          }),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          if (_suggested.isNotEmpty) ...[
            const SizedBox(height: 10),
            Card(
              child: Padding(
                padding: const EdgeInsets.all(UiTokens.padCard),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(Icons.people_alt_outlined,
                            size: 18,
                            color: Theme.of(context).colorScheme.primary),
                        const SizedBox(width: 8),
                        Text(context.l10n.suggestedTitle,
                            style:
                                const TextStyle(fontWeight: FontWeight.w800)),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Text(
                      context.l10n.suggestedHint,
                      style: TextStyle(
                        fontSize: 11.5,
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                      ),
                    ),
                    const SizedBox(height: 10),
                    SizedBox(
                      height: 138,
                      child: ListView.separated(
                        scrollDirection: Axis.horizontal,
                        itemCount: _suggested.length,
                        separatorBuilder: (_, __) => const SizedBox(width: 10),
                        itemBuilder: (context, index) {
                          final profile = _suggested[index];
                          final followed = _followed.contains(profile.id);
                          return Container(
                            width: 168,
                            padding: const EdgeInsets.all(10),
                            decoration: BoxDecoration(
                              color: Theme.of(context)
                                  .colorScheme
                                  .surfaceContainerHighest
                                  .withAlpha(110),
                              borderRadius: BorderRadius.circular(14),
                              border: Border.all(
                                color: Theme.of(context)
                                    .colorScheme
                                    .outlineVariant,
                              ),
                            ),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                StatusAvatar(
                                  imageUrl: profile.iconUrl,
                                  size: 38,
                                  showStatus: true,
                                  statusKey: profile.preferredUsername,
                                ),
                                const SizedBox(height: 6),
                                Text(
                                  profile.displayName.trim().isEmpty
                                      ? profile.preferredUsername
                                      : profile.displayName,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(
                                      fontWeight: FontWeight.w700,
                                      fontSize: 12.5),
                                ),
                                Text(
                                  '@${profile.preferredUsername}',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 11,
                                    color: Theme.of(context)
                                        .colorScheme
                                        .onSurfaceVariant,
                                  ),
                                ),
                                const Spacer(),
                                SizedBox(
                                  width: double.infinity,
                                  child: FilledButton.tonal(
                                    style: FilledButton.styleFrom(
                                      visualDensity: VisualDensity.compact,
                                      padding: const EdgeInsets.symmetric(
                                          horizontal: 8),
                                    ),
                                    onPressed: followed
                                        ? null
                                        : () => _follow(profile),
                                    child: Text(
                                      followed
                                          ? context.l10n.profileFollowing
                                          : context.l10n.settingsFollow,
                                      style: const TextStyle(fontSize: 11.5),
                                    ),
                                  ),
                                ),
                              ],
                            ),
                          );
                        },
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ],
      ),
    );
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
    this.header,
  });

  final AppState appState;
  final CoreApi api;
  final String kind;
  final String headerTitle;
  final String headerTooltip;
  final bool constrainWidth;
  final Widget? header;

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
  final _pending = <Map<String, dynamic>>[];
  final _scroll = ScrollController();
  Timer? _poll;
  bool _polling = false;
  StreamSubscription<bool>? _activity;
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

  @override
  void initState() {
    super.initState();
    _refresh();
    // Live updates arrive through the core event stream; this periodic poll is
    // only a safety net, so keep it slow to spare battery and CPU.
    _poll =
        AppActivity.periodic(const Duration(seconds: 20), (_) => _pollNew());
    _activity = AppActivity.changes.listen((active) {
      if (active && mounted) _pollNew();
    });
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
    _activity?.cancel();
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
    if (_loading) return;
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
      if (mounted) setState(() => _error = e.toString());
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
      if (!mounted) return;
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
      if (!mounted) return;
      setState(() => _error = msg);
      _scheduleRetryIfOffline(msg);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _pollNew() async {
    if (!mounted) return;
    if (_loading || _polling) return;
    if (!widget.appState.isRunning) return;
    _polling = true;
    try {
      final resp = await widget.api.fetchTimeline(widget.kind, limit: 20);
      final items = (resp['items'] as List<dynamic>? ?? [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .where(_isTimelineItem)
          .toList();
      if (!mounted || items.isEmpty) return;

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
        if (atTop) {
          _items.insertAll(0, fresh);
          _sortActivities(_items);
        } else {
          _pending.addAll(fresh);
          _sortActivities(_pending);
        }
      });
    } catch (_) {
      // best-effort auto refresh: ignore transient failures
    } finally {
      _polling = false;
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
    final list = RefreshIndicator(
      onRefresh: _refresh,
      child: ListView.builder(
        key: PageStorageKey('timeline-${widget.kind}'),
        controller: _scroll,
        padding:
            EdgeInsets.fromLTRB(16, widget.constrainWidth ? 4 : 16, 16, 20),
        scrollCacheExtent: const ScrollCacheExtent.pixels(1200),
        addAutomaticKeepAlives: false,
        itemCount: filtered.length + 3,
        itemBuilder: (context, index) {
          if (index == 0) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Container(
                  padding:
                      EdgeInsets.only(bottom: widget.constrainWidth ? 2 : 10),
                  margin:
                      EdgeInsets.only(bottom: widget.constrainWidth ? 6 : 10),
                  decoration: BoxDecoration(
                    border: Border(
                      bottom: BorderSide(
                        color: Theme.of(context).colorScheme.outlineVariant,
                      ),
                    ),
                  ),
                  child: IconButtonTheme(
                    data: const IconButtonThemeData(
                      style: ButtonStyle(
                        minimumSize: WidgetStatePropertyAll(Size(36, 36)),
                        iconSize: WidgetStatePropertyAll(20),
                        visualDensity: VisualDensity.compact,
                      ),
                    ),
                    child: Row(
                      children: [
                        if (!widget.constrainWidth) ...[
                          Container(
                            width: 8,
                            height: 8,
                            decoration: const BoxDecoration(
                              shape: BoxShape.circle,
                              gradient: LinearGradient(
                                colors: [
                                  Color(UiTokens.brandViolet),
                                  Color(UiTokens.brandCyan),
                                ],
                              ),
                            ),
                          ),
                          const SizedBox(width: 8),
                          Tooltip(
                            message: widget.headerTooltip,
                            child: Text(
                              widget.headerTitle,
                              style: Theme.of(context)
                                  .textTheme
                                  .titleSmall
                                  ?.copyWith(fontWeight: FontWeight.w800),
                            ),
                          ),
                        ],
                        const Spacer(),
                        IconButton(
                          tooltip: context.l10n.timelineFiltersTooltip,
                          onPressed: () => _showFilters(context),
                          icon: const Icon(Icons.filter_alt_outlined),
                        ),
                        IconButton(
                          tooltip: context.l10n.timelineRefreshHint,
                          onPressed: widget.appState.isRunning && !_syncing
                              ? _forceSyncAndRefresh
                              : null,
                          icon: _syncing
                              ? const SizedBox(
                                  width: 18,
                                  height: 18,
                                  child:
                                      CircularProgressIndicator(strokeWidth: 2),
                                )
                              : const Icon(Icons.refresh),
                        ),
                      ],
                    ),
                  ),
                ),
                if (widget.header != null) widget.header!,
              ],
            );
          }
          if (index == 1) {
            if (pendingCount > 0) {
              return Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: Card(
                  child: ListTile(
                    title: Text(context.l10n.timelineNewPosts(pendingCount)),
                    subtitle: Text(context.l10n.timelineShowNewPostsHint),
                    trailing: FilledButton(
                      onPressed: _applyPending,
                      child: Text(context.l10n.timelineShowNewPosts),
                    ),
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
                child: EmptyState(
                  icon: Icons.dynamic_feed_rounded,
                  title: context.l10n.listNoItems,
                  message: context.l10n.timelineEmptyHint,
                ),
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
            if (filtered.isEmpty && !_loading) return const SizedBox.shrink();
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
          return TimelineActivityCard(
            key: ValueKey(
                _activityId(item).isNotEmpty ? _activityId(item) : index),
            appState: widget.appState,
            activity: item,
          );
        },
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
    if (!_filterMedia &&
        !_filterReplies &&
        !_filterBoosts &&
        !_filterMentions) {
      return true;
    }
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
        ? (noteMap?['tag'] as List)
            .whereType<Map>()
            .any((m) => m['type'] == 'Mention')
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
                        child: Text(context.l10n.timelineFiltersTitle,
                            style:
                                const TextStyle(fontWeight: FontWeight.w800))),
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

class _TimelineSkeletonCard extends StatefulWidget {
  const _TimelineSkeletonCard();

  @override
  State<_TimelineSkeletonCard> createState() => _TimelineSkeletonCardState();
}

class _TimelineSkeletonCardState extends State<_TimelineSkeletonCard>
    with SingleTickerProviderStateMixin {
  late final AnimationController _shimmer = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1500),
  )..repeat();

  @override
  void dispose() {
    _shimmer.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final base = scheme.surfaceContainerHighest.withAlpha(120);
    final highlight = scheme.surfaceContainerHighest.withAlpha(50);
    return Card(
      child: AnimatedBuilder(
        animation: _shimmer,
        builder: (context, child) {
          final dx = -1.6 + 3.2 * _shimmer.value;
          return ShaderMask(
            blendMode: BlendMode.srcATop,
            shaderCallback: (bounds) => LinearGradient(
              begin: Alignment(dx - 0.7, 0),
              end: Alignment(dx + 0.7, 0),
              colors: [base, highlight, base],
              stops: const [0.0, 0.5, 1.0],
            ).createShader(bounds),
            child: child,
          );
        },
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
                  const SizedBox(width: 16),
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
      ),
    );
  }
}
