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
import '../../state/draft_store.dart';
import '../screens/compose_screen.dart';
import '../screens/note_detail_screen.dart';
import '../screens/profile_screen.dart';
import '../theme/ui_tokens.dart';
import '../utils/time_ago.dart';
import 'inline_composer.dart';
import 'status_avatar.dart';

class RightSidebar extends StatefulWidget {
  const RightSidebar({super.key, required this.appState});

  final AppState appState;

  @override
  State<RightSidebar> createState() => _RightSidebarState();
}

class _RightSidebarState extends State<RightSidebar> {
  Timer? _poll;
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  CoreConfig? _streamConfig;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;

  bool _loading = false;
  List<Map<String, dynamic>> _items = const [];
  String _draftKey = '';
  ComposeDraft? _draft;
  ActorProfile? _selfProfile;

  @override
  void initState() {
    super.initState();
    _poll = Timer.periodic(const Duration(seconds: 10), (_) => _refresh());
    WidgetsBinding.instance.addPostFrameCallback((_) => _refresh());
    _lastRunning = widget.appState.isRunning;
    _appStateListener = () {
      final running = widget.appState.isRunning;
      final cfg = widget.appState.config;
      final configChanged = !identical(_streamConfig, cfg);
      if (!running) {
        _stopStream();
      } else if (running && (!_lastRunning || configChanged)) {
        _startStream();
        _refresh();
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);
    if (widget.appState.isRunning) {
      _startStream();
    }
    _loadSelfProfile();
  }

  @override
  void dispose() {
    _poll?.cancel();
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
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
      _streamDebounce?.cancel();
      _streamDebounce = Timer(const Duration(milliseconds: 450), () {
        if (!mounted) return;
        _refresh();
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
    if (_loading) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (!widget.appState.isRunning) return;
    setState(() => _loading = true);
    try {
      final api = CoreApi(config: cfg);
      final resp = await api.fetchNotifications(limit: 12);
      final items = (resp['items'] as List<dynamic>? ?? const [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .toList();
      if (!mounted) return;
      setState(() => _items = items);
    } catch (_) {
      // best-effort
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _loadSelfProfile() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final base = cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = cfg.username.trim();
    if (base.isEmpty || user.isEmpty) return;
    final actorUrl = '$base/users/$user';
    final profile = await ActorRepository.instance.getActor(actorUrl);
    if (!mounted) return;
    setState(() => _selfProfile = profile);
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final cfg = widget.appState.config;
        final lastSeen = widget.appState.prefs.lastNotificationsSeenMs;
        final api = cfg == null ? null : CoreApi(config: cfg);
        _ensureDraft(cfg);

        return ListView(
          padding: const EdgeInsets.fromLTRB(UiTokens.padCard, UiTokens.padCard, UiTokens.padScreen, UiTokens.padCard),
          children: [
            if (cfg != null) ...[
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(UiTokens.padCard),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(
                        children: [
                          Text(context.l10n.composeTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
                          const Spacer(),
                          FilledButton.icon(
                            onPressed: api == null ? null : () => _openComposerOverlay(context, api),
                            icon: const Icon(Icons.edit_note),
                            label: Text(context.l10n.composeExpand),
                          ),
                        ],
                      ),
                      const SizedBox(height: UiTokens.gapSm),
                      Text(
                        context.l10n.composeQuickHint,
                        style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12),
                      ),
                      if (_draft != null) ...[
                        const SizedBox(height: UiTokens.gapSm),
                        Row(
                          children: [
                            Icon(Icons.description_outlined, size: 16, color: Theme.of(context).colorScheme.primary),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                context.l10n.composeDraftSaved,
                                style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
                              ),
                            ),
                            Tooltip(
                              message: context.l10n.composeDraftResumeTooltip,
                              child: IconButton(
                                onPressed: () => _resumeDraft(context),
                                icon: const Icon(Icons.history_edu),
                              ),
                            ),
                            Tooltip(
                              message: context.l10n.composeDraftDeleteTooltip,
                              child: IconButton(
                                onPressed: _clearDraft,
                                icon: const Icon(Icons.delete_outline),
                              ),
                            ),
                          ],
                        ),
                      ],
                    ],
                  ),
                ),
              ),
              const SizedBox(height: UiTokens.gapMd),
            ],
            Card(
              child: Padding(
                padding: const EdgeInsets.all(UiTokens.padCard),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Row(
                      children: [
                        Text(context.l10n.notificationsTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
                        const Spacer(),
                        if (_loading) const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)),
                        IconButton(
                          tooltip: context.l10n.timelineRefreshTitle,
                          onPressed: _loading ? null : _refresh,
                          icon: const Icon(Icons.refresh, size: 18),
                        ),
                      ],
                    ),
                    const SizedBox(height: UiTokens.gapSm),
                    if (!widget.appState.isRunning)
                      Text(context.l10n.notificationsCoreNotRunning, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12))
                    else if (_items.isEmpty)
                      Text(context.l10n.notificationsEmpty, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12))
                    else
                      Column(
                        children: [
                          for (final it in _items.take(8))
                            _SidebarNotificationRow(appState: widget.appState, item: it, lastSeenMs: lastSeen),
                        ],
                      ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: UiTokens.gapMd),
            Card(
              child: ListTile(
                title: Text(context.l10n.settingsAccount),
                subtitle: Text(cfg == null ? context.l10n.statusUnknownShort : '${cfg.username}@${cfg.domain}'),
                leading: StatusAvatar(
                  imageUrl: _selfProfile?.iconUrl ?? '',
                  size: 36,
                  showStatus: _selfProfile?.isFedi3 == true,
                  statusKey: _selfProfile?.statusKey,
                ),
                trailing: const Icon(Icons.person_outline),
                onTap: () {
                  final profile = _selfProfile;
                  final actorUrl = profile?.id.trim() ?? '';
                  if (actorUrl.isEmpty) return;
                  Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (_) => ProfileScreen(appState: widget.appState, actorUrl: actorUrl),
                    ),
                  );
                },
              ),
            ),
            const SizedBox(height: UiTokens.gapMd),
            Card(
              child: ListTile(
                title: Text(context.l10n.relaysTitle),
                subtitle: Text(cfg?.publicBaseUrl ?? ''),
              ),
            ),
          ],
        );
      },
    );
  }

  Future<void> _openComposerOverlay(BuildContext context, CoreApi api) async {
    await showDialog<void>(
      context: context,
      barrierDismissible: true,
      builder: (dialogContext) {
        final size = MediaQuery.of(dialogContext).size;
        final maxWidth = size.width < 640 ? size.width - 32 : 620.0;
        final maxHeight = size.height * 0.9;
        return Dialog(
          insetPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 20),
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: maxWidth, maxHeight: maxHeight),
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(UiTokens.padCard),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(context.l10n.composeTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
                      ),
                      IconButton(
                        tooltip: context.l10n.cancel,
                        onPressed: () => Navigator.of(dialogContext).pop(),
                        icon: const Icon(Icons.close),
                      ),
                    ],
                  ),
                  const SizedBox(height: UiTokens.gapSm),
                  InlineComposer(
                    appState: widget.appState,
                    api: api,
                    onPosted: () {
                      _refresh();
                      _loadDraft();
                      Navigator.of(dialogContext).maybePop();
                    },
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  void _ensureDraft(config) {
    if (config == null) return;
    final key = '${config.username}@${config.domain}';
    if (key == _draftKey) return;
    _draftKey = key;
    _loadDraft();
  }

  Future<void> _loadDraft() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final draft = await DraftStore.read(username: cfg.username, domain: cfg.domain);
    if (!mounted) return;
    setState(() {
      if (draft == null || draft.text.trim().isEmpty) {
        _draft = null;
      } else {
        _draft = draft;
      }
    });
  }

  void _resumeDraft(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute(builder: (_) => ComposeScreen(appState: widget.appState)),
    );
  }

  Future<void> _clearDraft() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    await DraftStore.clear(username: cfg.username, domain: cfg.domain);
    if (!mounted) return;
    setState(() => _draft = null);
  }
}

class _SidebarNotificationRow extends StatefulWidget {
  const _SidebarNotificationRow({required this.appState, required this.item, required this.lastSeenMs});

  final AppState appState;
  final Map<String, dynamic> item;
  final int lastSeenMs;

  @override
  State<_SidebarNotificationRow> createState() => _SidebarNotificationRowState();
}

class _SidebarNotificationRowState extends State<_SidebarNotificationRow> {
  ActorProfile? _actor;
  String _actorUrl = '';

  @override
  void initState() {
    super.initState();
    _loadActor();
  }

  @override
  void didUpdateWidget(covariant _SidebarNotificationRow oldWidget) {
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
    final actor = (activity['actor'] as String?)?.trim() ?? '';
    final noteActivity = _extractNoteActivity(activity);
    final canOpen = noteActivity != null || actor.isNotEmpty;

    final when = ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts).toLocal() : null;
    final isNew = ts > 0 && ts > widget.lastSeenMs;

    IconData icon = Icons.notifications;
    if (type == 'Follow') icon = Icons.person_add_alt_1;
    if (type == 'Accept') icon = Icons.check_circle;
    if (type == 'Reject') icon = Icons.person_remove;
    if (type == 'Like') icon = Icons.favorite;
    if (type == 'EmojiReact') icon = Icons.add_reaction;
    if (type == 'Announce') icon = Icons.repeat;
    if (type == 'Create') icon = Icons.reply;

    String label = context.l10n.notificationsGeneric;
    if (type == 'Follow') label = context.l10n.notificationsFollow;
    if (type == 'Accept') label = context.l10n.notificationsFollowAccepted;
    if (type == 'Reject') label = context.l10n.notificationsFollowRejected;
    if (type == 'Like') label = context.l10n.notificationsLike;
    if (type == 'EmojiReact') label = context.l10n.notificationsReact;
    if (type == 'Announce') label = context.l10n.notificationsBoost;
    if (type == 'Create') label = context.l10n.notificationsMentionOrReply;

    final actorName = _actor?.displayName ?? _shortActor(actor);
    final summary = _notificationSummary(activity);

    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: InkWell(
        borderRadius: BorderRadius.circular(10),
        onTap: canOpen ? () => _openTarget(context, noteActivity, actor) : null,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (isNew)
              Container(
                width: 8,
                height: 8,
                margin: const EdgeInsets.only(top: 12),
                decoration: BoxDecoration(color: Theme.of(context).colorScheme.primary, borderRadius: BorderRadius.circular(99)),
              )
            else
              const SizedBox(width: 8),
            const SizedBox(width: 8),
            Stack(
              clipBehavior: Clip.none,
              children: [
                StatusAvatar(
                  imageUrl: _actor?.iconUrl ?? '',
                  size: 28,
                  showStatus: _actor?.isFedi3 == true,
                  statusKey: _actor?.statusKey,
                ),
                Positioned(
                  right: -3,
                  bottom: -3,
                  child: Container(
                    padding: const EdgeInsets.all(3),
                    decoration: BoxDecoration(
                      color: _badgeColorFor(context, type),
                      shape: BoxShape.circle,
                      border: Border.all(color: Theme.of(context).colorScheme.surface, width: 2),
                    ),
                    child: Icon(icon, size: 10, color: Theme.of(context).colorScheme.onPrimary),
                  ),
                ),
              ],
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(icon, size: 14),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          actorName.isNotEmpty ? '$actorName · $label' : label,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
                        ),
                      ),
                      if (when != null)
                        Text(
                          formatTimeAgo(context, when),
                          style: TextStyle(fontSize: 11, color: Theme.of(context).colorScheme.onSurface.withAlpha(128)),
                        ),
                    ],
                  ),
                  if (summary.isNotEmpty) ...[
                    const SizedBox(height: 4),
                    Text(
                      summary,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12, color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  static String _shortActor(String actor) {
    final uri = Uri.tryParse(actor);
    if (uri == null || uri.host.isEmpty) return actor;
    if (uri.pathSegments.isNotEmpty && uri.pathSegments.first == 'users' && uri.pathSegments.length >= 2) {
      return '@${uri.pathSegments[1]}@${uri.host}';
    }
    return '@${uri.host}';
  }

  static String _normalizeNotificationType(Map<String, dynamic> activity) {
    final type = (activity['type'] as String?)?.trim() ?? '';
    if (type == 'Create') {
      final obj = activity['object'];
      if (obj is Map) {
        final inner = (obj['type'] as String?)?.trim() ?? '';
        if (inner == 'Follow' ||
            inner == 'Accept' ||
            inner == 'Reject' ||
            inner == 'Announce' ||
            inner == 'Like' ||
            inner == 'EmojiReact') {
          return inner;
        }
      }
    }
    if (type == 'Announce') {
      final obj = activity['object'];
      if (obj is Map) {
        final inner = (obj['type'] as String?)?.trim() ?? '';
        if (inner == 'Follow' || inner == 'Accept' || inner == 'Reject' || inner == 'Like' || inner == 'EmojiReact') {
          return inner;
        }
      }
    }
    return type;
  }
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
