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
import '../../services/actor_repository.dart';
import '../../services/core_event_stream.dart';
import '../../state/app_state.dart';
import '../screens/note_detail_screen.dart';
import '../screens/profile_screen.dart';
import '../utils/error_humanizer.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
import '../widgets/timeline_activity_card.dart';
import '../utils/time_ago.dart';

class NotificationsScreen extends StatefulWidget {
  const NotificationsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<NotificationsScreen> createState() => _NotificationsScreenState();
}

class _NotificationsScreenState extends State<NotificationsScreen> {
  bool _loading = false;
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
        if (mounted) _refresh();
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
        if (widget.appState.isRunning) _refresh();
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

  Future<void> _markSeenNow() async {
    final now = DateTime.now().millisecondsSinceEpoch;
    final prefs = widget.appState.prefs;
    await widget.appState.savePrefs(prefs.copyWith(lastNotificationsSeenMs: now));
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
      if (msg.contains('SocketException') || msg.contains('Connection refused') || msg.contains('errno = 1225')) {
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
      if (mounted) setState(() => _loading = false);
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

  bool _matchesFilter(Map<String, dynamic> item) {
    if (_filter == 'all') return true;
    final activity = (item['activity'] is Map) ? (item['activity'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
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
                        Text(context.l10n.notificationsCoreNotRunning, style: const TextStyle(fontWeight: FontWeight.w800)),
                        const SizedBox(height: 12),
                        FilledButton(
                          onPressed: () async {
                            await widget.appState.startCore();
                            if (!mounted) return;
                            if (widget.appState.isRunning) {
                              await _refresh();
                            } else if (widget.appState.lastError != null) {
                              setState(() => _error = widget.appState.lastError);
                            }
                          },
                          child: Text(context.l10n.coreStart),
                        ),
                        if (_error != null) ...[
                          const SizedBox(height: 10),
                          Text(
                            humanizeError(context, _error!),
                            style: TextStyle(color: Theme.of(context).colorScheme.error),
                          ),
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
        final panelBorder = Theme.of(context).colorScheme.outlineVariant.withAlpha(90);
        return Scaffold(
          appBar: AppBar(
            title: Text(context.l10n.notificationsTitle),
            actions: [
              IconButton(
                tooltip: 'Mark all read',
                onPressed: () async {
                  await _markSeenNow();
                  widget.appState.clearUnreadNotifications();
                  if (mounted) setState(() {});
                },
                icon: const Icon(Icons.mark_email_read_outlined),
              ),
              IconButton(onPressed: _loading ? null : _refresh, icon: const Icon(Icons.refresh)),
            ],
          ),
          body: RefreshIndicator(
            onRefresh: _refresh,
            child: ListView(
              padding: const EdgeInsets.all(12),
              children: [
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.surfaceContainerLow,
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(color: panelBorder),
                  ),
                  child: Wrap(
                    spacing: 8,
                    runSpacing: 6,
                    children: [
                      ChoiceChip(
                        label: const Text('All'),
                        selected: _filter == 'all',
                        onSelected: (_) => setState(() => _filter = 'all'),
                      ),
                      ChoiceChip(
                        label: const Text('Mentions'),
                        selected: _filter == 'mentions',
                        onSelected: (_) => setState(() => _filter = 'mentions'),
                      ),
                      ChoiceChip(
                        label: const Text('Reactions'),
                        selected: _filter == 'reactions',
                        onSelected: (_) => setState(() => _filter = 'reactions'),
                      ),
                      ChoiceChip(
                        label: const Text('Follows'),
                        selected: _filter == 'follows',
                        onSelected: (_) => setState(() => _filter = 'follows'),
                      ),
                      ChoiceChip(
                        label: const Text('Boosts'),
                        selected: _filter == 'boosts',
                        onSelected: (_) => setState(() => _filter = 'boosts'),
                      ),
                      ChoiceChip(
                        label: const Text('Likes'),
                        selected: _filter == 'likes',
                        onSelected: (_) => setState(() => _filter = 'likes'),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 10),
                if (_error != null)
                  NetworkErrorCard(
                    message: _error,
                    onRetry: _refresh,
                    compact: true,
                  ),
                if (filtered.isEmpty && !_loading && _error == null)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 24),
                    child: Center(child: Text(context.l10n.notificationsEmpty)),
                  ),
                for (final it in filtered) ...[
                  _NotificationItem(appState: widget.appState, item: it),
                ],
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 12),
                  child: Center(
                    child: _loading
                        ? const CircularProgressIndicator()
                        : OutlinedButton(
                            onPressed: canLoadMore ? _loadMore : null,
                            child: Text(canLoadMore ? context.l10n.listLoadMore : context.l10n.listEnd),
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

class _NotificationItem extends StatefulWidget {
  const _NotificationItem({required this.appState, required this.item});

  final AppState appState;
  final Map<String, dynamic> item;

  @override
  State<_NotificationItem> createState() => _NotificationItemState();
}

class _NotificationItemState extends State<_NotificationItem> {
  ActorProfile? _actor;
  String _actorUrl = '';

  @override
  void initState() {
    super.initState();
    _loadActor();
  }

  @override
  void didUpdateWidget(covariant _NotificationItem oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.item != widget.item) {
      _loadActor();
    }
  }

  Future<void> _loadActor() async {
    final activity = (widget.item['activity'] is Map) ? (widget.item['activity'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
    final actorUrl = (activity['actor'] as String?)?.trim() ?? '';
    if (actorUrl.isEmpty || actorUrl == _actorUrl) return;
    _actorUrl = actorUrl;
    final profile = await ActorRepository.instance.getActor(actorUrl);
    if (!mounted) return;
    setState(() => _actor = profile);
  }

  void _openTarget(BuildContext context, Map<String, dynamic>? noteActivity, String actorUrl) {
    if (noteActivity != null) {
      Navigator.of(context).push(
        MaterialPageRoute(builder: (_) => NoteDetailScreen(appState: widget.appState, activity: noteActivity)),
      );
      return;
    }
    if (actorUrl.isNotEmpty) {
      Navigator.of(context).push(
        MaterialPageRoute(builder: (_) => ProfileScreen(appState: widget.appState, actorUrl: actorUrl)),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final item = widget.item;
    final ts = (item['ts'] is num) ? (item['ts'] as num).toInt() : int.tryParse(item['ts']?.toString() ?? '') ?? 0;
    final activity = (item['activity'] is Map) ? (item['activity'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
    final type = _normalizeNotificationType(activity);
    final actorUrl = (activity['actor'] as String?)?.trim() ?? '';
    final label = _labelFor(context, type);
    final when = ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts).toLocal() : null;
    final actorName = _actor?.displayName ?? _shortActor(actorUrl);
    final actorHandle = actorName.isEmpty
        ? ''
        : (_actor?.preferredUsername.isNotEmpty == true ? _actor!.preferredUsername : _shortActor(actorUrl));
    final summary = _notificationSummary(activity);
    final noteActivity = _extractNoteActivity(activity);

    final iconData = _iconDataFor(type);
    final canOpen = noteActivity != null || actorUrl.isNotEmpty;
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            borderRadius: BorderRadius.circular(14),
            onTap: canOpen ? () => _openTarget(context, noteActivity, actorUrl) : null,
            child: Card(
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Stack(
                      clipBehavior: Clip.none,
                      children: [
                        StatusAvatar(
                          imageUrl: _actor?.iconUrl ?? '',
                          size: 36,
                          showStatus: _actor?.isFedi3 == true,
                          statusKey: _actor?.statusKey,
                        ),
                        Positioned(
                          right: -3,
                          bottom: -3,
                          child: Container(
                            padding: const EdgeInsets.all(4),
                            decoration: BoxDecoration(
                              color: _badgeColorFor(context, type),
                              shape: BoxShape.circle,
                              border: Border.all(color: Theme.of(context).colorScheme.surface, width: 2),
                            ),
                            child: Icon(iconData, size: 10, color: Theme.of(context).colorScheme.onPrimary),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Icon(iconData, size: 16),
                              const SizedBox(width: 6),
                              Expanded(
                                child: Text.rich(
                                  actorName.isNotEmpty
                                      ? TextSpan(
                                          children: [
                                            TextSpan(text: actorName, style: const TextStyle(fontWeight: FontWeight.w800)),
                                            TextSpan(text: ' · $label'),
                                          ],
                                        )
                                      : TextSpan(text: label, style: const TextStyle(fontWeight: FontWeight.w800)),
                                ),
                              ),
                              if (when != null)
                                Text(
                                  formatTimeAgo(context, when),
                                  style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
                                ),
                            ],
                          ),
                          if (actorHandle.isNotEmpty) ...[
                            const SizedBox(height: 4),
                            Text(
                              actorHandle,
                              style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160), fontSize: 12),
                            ),
                          ],
                          if (summary.isNotEmpty) ...[
                            const SizedBox(height: 6),
                            Container(
                              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                              decoration: BoxDecoration(
                                color: Theme.of(context).colorScheme.surfaceContainerHigh.withAlpha(90),
                                borderRadius: BorderRadius.circular(10),
                                border: Border(left: BorderSide(color: Theme.of(context).colorScheme.primary.withAlpha(140), width: 2)),
                              ),
                              child: Text(
                                summary,
                                maxLines: 3,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179)),
                              ),
                            ),
                          ],
                          if (type == 'Follow' || type == 'Create') ...[
                            const SizedBox(height: 8),
                            Row(
                              children: [
                                if (type == 'Follow' && actorUrl.isNotEmpty)
                                  TextButton(
                                    onPressed: () async {
                                      final api = CoreApi(config: widget.appState.config!);
                                      try {
                                        await api.follow(actorUrl);
                                        if (!context.mounted) return;
                                        ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Followed back')));
                                      } catch (e) {
                                        if (!context.mounted) return;
                                        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Follow back failed: $e')));
                                      }
                                    },
                                    child: const Text('Follow back'),
                                  ),
                                if (type == 'Create')
                                  TextButton(
                                    onPressed: () {
                                      Navigator.of(context).push(
                                        MaterialPageRoute(builder: (_) => NoteDetailScreen(appState: widget.appState, activity: activity)),
                                      );
                                    },
                                    child: const Text('Reply'),
                                  ),
                              ],
                            ),
                          ],
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          if (noteActivity != null) ...[
            const SizedBox(height: 6),
            TimelineActivityCard(appState: widget.appState, activity: noteActivity, elevated: true),
          ],
        ],
      ),
    );
  }

  IconData _iconDataFor(String type) {
    return switch (type) {
      'Follow' => Icons.person_add_alt,
      'Accept' => Icons.person_add_alt_1,
      'Reject' => Icons.person_remove,
      'Like' => Icons.favorite,
      'EmojiReact' => Icons.emoji_emotions,
      'Announce' => Icons.repeat,
      'Create' => Icons.alternate_email,
      _ => Icons.notifications,
    };
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

}

Color _badgeColorFor(BuildContext context, String type) {
  final scheme = Theme.of(context).colorScheme;
  return switch (type) {
    'Like' => scheme.secondary,
    'EmojiReact' => scheme.tertiary,
    'Reject' => scheme.error,
    'Announce' => scheme.primary,
    'Create' => scheme.primary,
    _ => scheme.primary,
  };
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

Map<String, dynamic>? _extractNoteActivity(Map<String, dynamic> activity) {
  final type = (activity['type'] as String?)?.trim() ?? '';
  if (type == 'Create' || type == 'Announce' || type == 'Update') return activity;
  final obj = activity['object'];
  if (obj is Map) {
    final map = obj.cast<String, dynamic>();
    if (_isNoteLikeType(map['type'])) return map;
    final inner = map['object'];
    if (inner is Map) {
      final innerMap = inner.cast<String, dynamic>();
      if (_isNoteLikeType(innerMap['type'])) return innerMap;
    }
  }
  return null;
}

String _notificationSummary(Map<String, dynamic> activity) {
  final noteActivity = _extractNoteActivity(activity);
  if (noteActivity != null) {
    final note = Note.tryParse(noteActivity);
    if (note != null) {
      final text = _stripHtml(note.contentHtml);
      if (text.isNotEmpty) return text;
    }
  }
  final obj = activity['object'];
  if (obj is String) return obj.trim();
  if (obj is Map) {
    final map = obj.cast<String, dynamic>();
    final summary = (map['summary'] as String?)?.trim() ?? '';
    if (summary.isNotEmpty) return _stripHtml(summary);
    final name = (map['name'] as String?)?.trim() ?? '';
    if (name.isNotEmpty) return _stripHtml(name);
    final id = (map['id'] as String?)?.trim() ?? '';
    if (id.isNotEmpty) return id;
  }
  return '';
}

String _stripHtml(String html) {
  var text = html;
  text = text.replaceAll(RegExp(r'<br\\s*/?>', caseSensitive: false), '\n');
  text = text.replaceAll(RegExp(r'</p>', caseSensitive: false), '\n');
  text = text.replaceAll(RegExp(r'<[^>]+>'), ' ');
  text = text.replaceAll('&amp;', '&');
  text = text.replaceAll('&quot;', '"');
  text = text.replaceAll('&#39;', "'");
  text = text.replaceAll('&lt;', '<');
  text = text.replaceAll('&gt;', '>');
  return text.replaceAll(RegExp(r'\\s+'), ' ').trim();
}

bool _isNoteLikeType(dynamic value) {
  final ty = value is String ? value.trim() : '';
  return ty == 'Note' || ty == 'Article' || ty == 'Question';
}

String _shortActor(String actor) {
  final uri = Uri.tryParse(actor);
  if (uri == null || uri.host.isEmpty) return actor;
  if (uri.pathSegments.isNotEmpty && uri.pathSegments.first == 'users' && uri.pathSegments.length >= 2) {
    return '@${uri.pathSegments[1]}@${uri.host}';
  }
  return '@${uri.host}';
}
