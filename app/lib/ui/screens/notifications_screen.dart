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
import '../screens/note_detail_screen.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/network_error_card.dart';
import '../widgets/timeline_activity_card.dart';

class NotificationsScreen extends StatefulWidget {
  const NotificationsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<NotificationsScreen> createState() => _NotificationsScreenState();
}

class _NotificationsScreenState extends State<NotificationsScreen> {
  bool _loading = false;
  bool _pendingIncremental = false;
  String? _error;
  String? _cursor;
  final _items = <Map<String, dynamic>>[];
  bool _coreUnreachable = false;
  String _filter = 'all';
  late bool _lastRunning;
  late final VoidCallback _appStateListener;
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  Timer? _loadRetry;
  CoreConfig? _streamConfig;

  @override
  void initState() {
    super.initState();
    widget.appState.clearUnreadNotifications();
    _lastRunning = widget.appState.isRunning;
    _appStateListener = () {
      final running = widget.appState.isRunning;
      final cfg = widget.appState.config;
      final configChanged = !identical(_streamConfig, cfg);
      if (!running) {
        _stopStream();
      }
      if (running && (!_lastRunning || configChanged)) {
        _coreUnreachable = false;
        if (mounted) {
          // H-53: keep the list and cursor when resuming; only the first load
          // resets the paging state.
          if (_items.isEmpty) {
            _refresh();
          } else {
            _refreshIncremental();
          }
        }
        _startStream();
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);

    if (widget.appState.isRunning) {
      _refresh();
      _startStream();
    }
  }

  @override
  void dispose() {
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
    _loadRetry?.cancel();
    _streamSub?.cancel();
    widget.appState.removeListener(_appStateListener);
    super.dispose();
  }

  void _startStream() {
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_streamConfig, cfg) && _streamSub != null) return;
    _streamConfig = cfg;
    _streamSub?.cancel();
    _streamSub = CoreEventStream(config: cfg).stream().listen((ev) {
      if (!mounted) return;
      if (ev.kind != 'notification' && ev.kind != 'inbox') return;
      // Debounce refresh bursts.
      _streamDebounce?.cancel();
      _streamDebounce = Timer(const Duration(milliseconds: 350), () {
        if (!mounted) return;
        if (widget.appState.isRunning) _refreshIncremental();
        widget.appState.incrementUnreadNotifications();
      });
    }, onError: (_) => _scheduleStreamRetry(), onDone: _scheduleStreamRetry);
  }

  void _stopStream() {
    _streamRetry?.cancel();
    _streamSub?.cancel();
    _streamSub = null;
  }

  void _scheduleStreamRetry() {
    if (!mounted) return;
    _streamSub = null;
    if (!widget.appState.isRunning) return;
    _streamRetry?.cancel();
    _streamRetry = Timer(const Duration(seconds: 2), () {
      if (!mounted) return;
      _startStream();
    });
  }

  Future<void> _refresh() async {
    setState(() {
      _items.clear();
      _cursor = null;
      _coreUnreachable = false;
    });
    await _loadMore();
    await _markSeenNow();
    widget.appState.clearUnreadNotifications();
  }

  /// H-53/AW-11: fetch page 1 and merge unseen items at the head without
  /// clearing the existing list or resetting the pagination cursor.
  Future<void> _refreshIncremental() async {
    if (_loading) {
      // H-53/AW-11: a notification can arrive while the first page is loading;
      // remember it so the merge is not silently dropped.
      _pendingIncremental = true;
      return;
    }
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    try {
      final resp = await api.fetchNotifications(limit: 50);
      final items = (resp['items'] as List<dynamic>? ?? const [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .toList();
      if (!mounted) return;
      setState(() {
        final known = _items
            .map((m) => m['id']?.toString() ?? '')
            .where((s) => s.isNotEmpty)
            .toSet();
        final fresh = items
            .where((m) => !known.contains(m['id']?.toString() ?? ''))
            .toList();
        _items.insertAll(0, fresh);
        final next = (resp['next'] as String?)?.trim();
        if (_cursor == null && next != null && next.isNotEmpty) {
          _cursor = next;
        }
      });
    } catch (_) {
      // Best-effort: keep showing the current list on transient failures.
    }
  }

  Future<void> _markSeenNow() async {
    final now = DateTime.now().millisecondsSinceEpoch;
    final prefs = widget.appState.prefs;
    await widget.appState
        .savePrefs(prefs.copyWith(lastNotificationsSeenMs: now));
  }

  Future<void> _loadMore() async {
    if (_loading) return;
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config!;
    final api = CoreApi(config: cfg);
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final resp = await api.fetchNotifications(cursor: _cursor, limit: 50);
      final items = (resp['items'] as List<dynamic>? ?? const [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .toList();
      final next = (resp['next'] as String?)?.trim();
      if (!mounted) return;
      setState(() {
        _items.addAll(items);
        _cursor = (next != null && next.isNotEmpty) ? next : null;
      });
    } catch (e) {
      final msg = e.toString();
      if (msg.contains('SocketException') ||
          msg.contains('Connection refused') ||
          msg.contains('errno = 1225')) {
        if (mounted) {
          setState(() {
            _coreUnreachable = true;
            _error = msg;
          });
        }
        _scheduleRetryIfOffline(msg);
        return;
      }
      if (!mounted) return;
      setState(() => _error = msg);
    } finally {
      if (mounted) {
        setState(() => _loading = false);
        if (_pendingIncremental) {
          _pendingIncremental = false;
          unawaited(_refreshIncremental());
        }
      }
    }
  }

  void _scheduleRetryIfOffline(String msg) {
    if (!mounted) return;
    if (!widget.appState.isRunning) return;
    final lower = msg.toLowerCase();
    final shouldRetry = lower.contains('socketexception') ||
        lower.contains('connection refused') ||
        lower.contains('errno = 111') ||
        lower.contains('errno=111') ||
        lower.contains('errno = 1225');
    if (!shouldRetry) return;
    if (_loadRetry != null && _loadRetry!.isActive) return;
    _loadRetry = Timer(const Duration(seconds: 1), () {
      if (!mounted) return;
      if (!widget.appState.isRunning) return;
      _refresh();
    });
  }


  List<Widget> _groupedNotifications(
      BuildContext context, List<Map<String, dynamic>> items) {
    final out = <Widget>[];
    String? last;
    for (final item in items) {
      final ts = (item['ts'] is num)
          ? (item['ts'] as num).toInt()
          : int.tryParse(item['ts']?.toString() ?? '') ?? 0;
      final bucket = _bucket(ts);
      if (bucket != last) {
        out.add(Padding(
          padding: const EdgeInsets.fromLTRB(4, 14, 4, 6),
          child: Text(
            bucket,
            style: Theme.of(context).textTheme.labelSmall?.copyWith(
                  letterSpacing: 1.1,
                  fontWeight: FontWeight.w800,
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
          ),
        ));
        last = bucket;
      }
      out.add(_NotificationItem(appState: widget.appState, item: item));
    }
    return out;
  }

  String _bucket(int ts) {
    if (ts <= 0) return context.l10n.notificationsEarlier;
    final when = DateTime.fromMillisecondsSinceEpoch(ts).toLocal();
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final day = DateTime(when.year, when.month, when.day);
    if (day == today) return context.l10n.notificationsToday;
    if (day == today.subtract(const Duration(days: 1))) {
      return context.l10n.notificationsYesterday;
    }
    return context.l10n.notificationsEarlier;
  }

  bool _matchesFilter(Map<String, dynamic> item) {
    if (_filter == 'all') return true;
    final activity = (item['activity'] is Map)
        ? (item['activity'] as Map).cast<String, dynamic>()
        : const <String, dynamic>{};
    final type = _normalizeNotificationType(activity);
    return switch (_filter) {
      'mentions' => type == 'Create',
      'reactions' => type == 'EmojiReact',
      'follows' => type == 'Follow' || type == 'Accept' || type == 'Reject',
      'boosts' => type == 'Announce',
      'likes' => type == 'Like',
      _ => true,
    };
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        if (!widget.appState.isRunning || _coreUnreachable) {
          return Scaffold(
            appBar: AppBar(title: Text(context.l10n.notificationsTitle)),
            body: Center(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Card(
                  child: Padding(
                    padding: const EdgeInsets.all(16),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(context.l10n.notificationsCoreNotRunning,
                            style:
                                const TextStyle(fontWeight: FontWeight.w800)),
                        const SizedBox(height: 12),
                        FilledButton(
                          onPressed: () async {
                            await widget.appState.startCore();
                            if (!mounted) return;
                            if (widget.appState.isRunning) {
                              await _refresh();
                            } else if (widget.appState.lastError != null) {
                              setState(
                                  () => _error = widget.appState.lastError);
                            }
                          },
                          child: Text(context.l10n.coreStart),
                        ),
                        if (_error != null) ...[
                          const SizedBox(height: 10),
                          Text(_error!,
                              style: TextStyle(
                                  color: Theme.of(context).colorScheme.error)),
                        ],
                      ],
                    ),
                  ),
                ),
              ),
            ),
          );
        }

        final canLoadMore = _cursor != null && !_loading;
        final filtered = _items.where(_matchesFilter).toList(growable: false);
        return Scaffold(
          appBar: AppBar(
            title: Text(context.l10n.notificationsTitle),
            actions: [
              IconButton(
                tooltip: context.l10n.notificationsMarkAllRead,
                onPressed: () async {
                  await _markSeenNow();
                  widget.appState.clearUnreadNotifications();
                  if (mounted) setState(() {});
                },
                icon: const Icon(Icons.mark_email_read_outlined),
              ),
              IconButton(
                  onPressed: _loading ? null : _refresh,
                  icon: const Icon(Icons.refresh)),
            ],
          ),
          body: RefreshIndicator(
            onRefresh: _refresh,
            child: ListView(
              padding: const EdgeInsets.all(12),
              children: [
                Wrap(
                  spacing: 8,
                  runSpacing: 6,
                  children: [
                    ChoiceChip(
                      label: Text(context.l10n.notificationsFilterAll),
                      selected: _filter == 'all',
                      onSelected: (_) => setState(() => _filter = 'all'),
                    ),
                    ChoiceChip(
                      label: Text(context.l10n.notificationsFilterMentions),
                      selected: _filter == 'mentions',
                      onSelected: (_) => setState(() => _filter = 'mentions'),
                    ),
                    ChoiceChip(
                      label: Text(context.l10n.notificationsFilterReactions),
                      selected: _filter == 'reactions',
                      onSelected: (_) => setState(() => _filter = 'reactions'),
                    ),
                    ChoiceChip(
                      label: Text(context.l10n.notificationsFilterFollows),
                      selected: _filter == 'follows',
                      onSelected: (_) => setState(() => _filter = 'follows'),
                    ),
                    ChoiceChip(
                      label: Text(context.l10n.notificationsFilterBoosts),
                      selected: _filter == 'boosts',
                      onSelected: (_) => setState(() => _filter = 'boosts'),
                    ),
                    ChoiceChip(
                      label: Text(context.l10n.notificationsFilterLikes),
                      selected: _filter == 'likes',
                      onSelected: (_) => setState(() => _filter = 'likes'),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                if (_error != null)
                  NetworkErrorCard(
                    message: _error,
                    onRetry: _refresh,
                    compact: true,
                  ),
                if (filtered.isEmpty && !_loading && _error == null)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 20),
                    child: EmptyState(
                      icon: Icons.notifications_none_rounded,
                      title: context.l10n.notificationsEmpty,
                    ),
                  ),
                ..._groupedNotifications(context, filtered),
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 12),
                  child: Center(
                    child: _loading
                        ? const CircularProgressIndicator()
                        : OutlinedButton(
                            onPressed: canLoadMore ? _loadMore : null,
                            child: Text(canLoadMore
                                ? context.l10n.listLoadMore
                                : context.l10n.listEnd),
                          ),
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _NotificationItem extends StatelessWidget {
  const _NotificationItem({required this.appState, required this.item});

  final AppState appState;
  final Map<String, dynamic> item;

  @override
  Widget build(BuildContext context) {
    final ts = (item['ts'] is num)
        ? (item['ts'] as num).toInt()
        : int.tryParse(item['ts']?.toString() ?? '') ?? 0;
    final activity = (item['activity'] is Map)
        ? (item['activity'] as Map).cast<String, dynamic>()
        : const <String, dynamic>{};
    final type = _normalizeNotificationType(activity);
    final actor = (activity['actor'] as String?)?.trim() ?? '';
    final label = _labelFor(context, type);
    final when =
        ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts).toLocal() : null;

    return Column(
      children: [
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    _iconFor(type),
                    const SizedBox(width: 8),
                    Expanded(
                        child: Text(label,
                            style:
                                const TextStyle(fontWeight: FontWeight.w800))),
                    if (when != null)
                      Text(
                        '${when.hour.toString().padLeft(2, '0')}:${when.minute.toString().padLeft(2, '0')}',
                        style: TextStyle(
                            color: Theme.of(context)
                                .colorScheme
                                .onSurface
                                .withAlpha(128),
                            fontSize: 12),
                      ),
                  ],
                ),
                const SizedBox(height: 10),
                if (type == 'Create')
                  TimelineActivityCard(
                      appState: appState, activity: activity, elevated: true)
                else
                  Text(
                    _shortText(activity),
                    style: TextStyle(
                        color: Theme.of(context)
                            .colorScheme
                            .onSurface
                            .withAlpha(179)),
                  ),
                if (type == 'Follow' || type == 'Create') ...[
                  const SizedBox(height: 8),
                  Row(
                    children: [
                      if (type == 'Follow' && actor.isNotEmpty)
                        TextButton(
                          onPressed: () async {
                            final api = CoreApi(config: appState.config!);
                            try {
                              await api.follow(actor);
                              if (!context.mounted) return;
                              ScaffoldMessenger.of(context).showSnackBar(
                                  SnackBar(
                                      content: Text(context
                                          .l10n.notificationsFollowedBack)));
                            } catch (e) {
                              if (!context.mounted) return;
                              ScaffoldMessenger.of(context).showSnackBar(
                                  SnackBar(
                                      content: Text(context.l10n
                                          .notificationsFollowBackFailed(
                                              e.toString()))));
                            }
                          },
                          child: Text(context.l10n.notificationsFollowBack),
                        ),
                      if (type == 'Create')
                        TextButton(
                          onPressed: () {
                            Navigator.of(context).push(
                              MaterialPageRoute(
                                  builder: (_) => NoteDetailScreen(
                                      appState: appState, activity: activity)),
                            );
                          },
                          child: Text(context.l10n.notificationsReply),
                        ),
                    ],
                  ),
                ],
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _iconFor(String type) {
    final icon = switch (type) {
      'Follow' => Icons.person_add_alt,
      'Accept' => Icons.person_add_alt_1,
      'Reject' => Icons.person_remove,
      'Like' => Icons.favorite,
      'EmojiReact' => Icons.emoji_emotions,
      'Announce' => Icons.repeat,
      'Create' => Icons.alternate_email,
      _ => Icons.notifications,
    };
    return Icon(icon, size: 18);
  }

  String _labelFor(BuildContext context, String type) {
    return switch (type) {
      'Follow' => context.l10n.notificationsFollow,
      'Accept' => context.l10n.notificationsFollowAccepted,
      'Reject' => context.l10n.notificationsFollowRejected,
      'Like' => context.l10n.notificationsLike,
      'EmojiReact' => context.l10n.notificationsReact,
      'Announce' => context.l10n.notificationsBoost,
      'Create' => context.l10n.notificationsMentionOrReply,
      _ => context.l10n.notificationsGeneric,
    };
  }

  String _shortText(Map<String, dynamic> activity) {
    final actor = (activity['actor'] as String?)?.trim() ?? '';
    final obj = activity['object'];
    if (obj is String) return '$actor → $obj';
    if (obj is Map) {
      final m = obj.cast<String, dynamic>();
      return '$actor → ${(m['id'] as String?)?.trim() ?? ''}';
    }
    return actor;
  }
}

String _normalizeNotificationType(Map<String, dynamic> activity) {
  final type = (activity['type'] as String?)?.trim() ?? '';
  final inner = _extractNestedType(activity['object']);
  if (inner.isEmpty) return type;
  if (type == 'Create') {
    if (inner == 'Note') return 'Create';
    return inner;
  }
  if (type == 'Announce') {
    if (inner == 'Note') return 'Announce';
    return inner;
  }
  return type;
}

String _extractNestedType(dynamic obj) {
  if (obj is! Map) return '';
  final map = obj.cast<String, dynamic>();
  final type = (map['type'] as String?)?.trim() ?? '';
  if (type.isNotEmpty) return type;
  final inner = map['object'];
  if (inner is Map) return _extractNestedType(inner);
  return '';
}
