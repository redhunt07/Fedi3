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
import '../../services/core_event_stream.dart';
import '../../state/app_state.dart';
import '../../state/draft_store.dart';
import '../screens/compose_screen.dart';
import '../screens/profile_screen.dart';
import '../screens/relays_screen.dart';
import '../screens/saved_screen.dart';
import '../screens/search_screen.dart';
import '../theme/ui_tokens.dart';
import 'fedi3_ui.dart';
import '../utils/time_ago.dart';

class RightSidebar extends StatefulWidget {
  const RightSidebar({super.key, required this.appState});

  final AppState appState;

  @override
  State<RightSidebar> createState() => _RightSidebarState();
}

class _RightSidebarState extends State<RightSidebar> {
  Timer? _poll;
  Timer? _trendingTimer;
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
  List<Map<String, dynamic>> _trending = const [];

  @override
  void initState() {
    super.initState();
    _poll =
        AppActivity.periodic(const Duration(seconds: 10), (_) => _refresh());
    _trendingTimer = AppActivity.periodic(
        const Duration(minutes: 5), (_) => _loadTrending());
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _refresh();
      _loadTrending();
    });
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
  }

  @override
  void dispose() {
    _poll?.cancel();
    _trendingTimer?.cancel();
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

  Future<void> _loadTrending() async {
    final cfg = widget.appState.config;
    if (cfg == null || !widget.appState.isRunning) return;
    try {
      final api = CoreApi(config: cfg);
      final resp = await api.fetchTrendingHashtags(limit: 8);
      final raw = resp['items'];
      final items = <Map<String, dynamic>>[];
      if (raw is List) {
        for (final it in raw) {
          if (it is! Map) continue;
          items.add(it.cast<String, dynamic>());
        }
      }
      if (!mounted) return;
      setState(() => _trending = items);
    } catch (_) {
      // Best-effort: keep the previous trending list.
    }
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
          padding: const EdgeInsets.fromLTRB(UiTokens.padCard, UiTokens.padCard,
              UiTokens.padScreen, UiTokens.padCard),
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
                          _ComposeAvatar(iconUrl: cfg.iconUrl, size: 40),
                          const SizedBox(width: 12),
                          Expanded(
                            child: Text(
                              context.l10n.composeQuickHint,
                              style: TextStyle(
                                color: Theme.of(context)
                                    .colorScheme
                                    .onSurface
                                    .withAlpha(190),
                                fontSize: 13,
                                height: 1.35,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: UiTokens.gapMd),
                      AuroraButton(
                        label: context.l10n.composeExpand,
                        icon: Icons.edit_rounded,
                        expand: true,
                        onPressed:
                            api == null ? null : () => _openComposer(context),
                      ),
                      if (_draft != null) ...[
                        const SizedBox(height: UiTokens.gapMd),
                        Container(
                          padding: const EdgeInsets.symmetric(
                              horizontal: 10, vertical: 8),
                          decoration: BoxDecoration(
                            color: Theme.of(context)
                                .colorScheme
                                .primary
                                .withAlpha(14),
                            borderRadius:
                                BorderRadius.circular(UiTokens.radiusCardSm),
                            border: Border.all(
                              color: Theme.of(context)
                                  .colorScheme
                                  .primary
                                  .withAlpha(46),
                            ),
                          ),
                          child: Row(
                            children: [
                              Icon(Icons.description_outlined,
                                  size: 16,
                                  color: Theme.of(context).colorScheme.primary),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  context.l10n.composeDraftSaved,
                                  style: const TextStyle(
                                      fontSize: 12,
                                      fontWeight: FontWeight.w700),
                                ),
                              ),
                              Tooltip(
                                message: context.l10n.composeDraftResumeTooltip,
                                child: IconButton(
                                  visualDensity: VisualDensity.compact,
                                  onPressed: () => _resumeDraft(context),
                                  icon: const Icon(Icons.history_edu, size: 18),
                                ),
                              ),
                              Tooltip(
                                message: context.l10n.composeDraftDeleteTooltip,
                                child: IconButton(
                                  visualDensity: VisualDensity.compact,
                                  onPressed: _clearDraft,
                                  icon: const Icon(Icons.delete_outline,
                                      size: 18),
                                ),
                              ),
                            ],
                          ),
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
                        Text(context.l10n.notificationsTitle,
                            style:
                                const TextStyle(fontWeight: FontWeight.w800)),
                        const Spacer(),
                        if (_loading)
                          const SizedBox(
                              width: 16,
                              height: 16,
                              child: CircularProgressIndicator(strokeWidth: 2)),
                        IconButton(
                          tooltip: context.l10n.timelineRefreshTitle,
                          onPressed: _loading ? null : _refresh,
                          icon: const Icon(Icons.refresh, size: 18),
                        ),
                      ],
                    ),
                    const SizedBox(height: UiTokens.gapSm),
                    if (!widget.appState.isRunning)
                      Text(context.l10n.notificationsCoreNotRunning,
                          style: TextStyle(
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurface
                                  .withAlpha(179),
                              fontSize: 12))
                    else if (_items.isEmpty)
                      Text(context.l10n.notificationsEmpty,
                          style: TextStyle(
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurface
                                  .withAlpha(179),
                              fontSize: 12))
                    else
                      Column(
                        children: [
                          for (final it in _items.take(8))
                            _SidebarNotificationRow(
                                item: it, lastSeenMs: lastSeen),
                        ],
                      ),
                  ],
                ),
              ),
            ),
            if (_trending.isNotEmpty) ...[
              const SizedBox(height: UiTokens.gapMd),
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(UiTokens.padCard),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Text(context.l10n.trendingTitle,
                              style:
                                  const TextStyle(fontWeight: FontWeight.w800)),
                          const Spacer(),
                          Icon(Icons.trending_up_rounded,
                              size: 18,
                              color: Theme.of(context).colorScheme.primary),
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
                                label: Text(
                                  '#$name',
                                  style: const TextStyle(fontSize: 12),
                                ),
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
            ],
            const SizedBox(height: UiTokens.gapMd),
            GlassPanel(
              padding: const EdgeInsets.all(6),
              child: Column(
                children: [
                  _SidebarAction(
                    title: context.l10n.settingsAccount,
                    subtitle: cfg == null
                        ? context.l10n.statusUnknownShort
                        : '${cfg.username}@${cfg.domain}',
                    leading: cfg == null
                        ? const Icon(Icons.person_outline)
                        : _ComposeAvatar(iconUrl: cfg.iconUrl, size: 30),
                    onTap: cfg == null
                        ? null
                        : () => Navigator.of(context).push(
                              MaterialPageRoute<void>(
                                builder: (_) => ProfileScreen(
                                  appState: widget.appState,
                                  actorUrl:
                                      '${cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '')}/users/${cfg.username.trim()}',
                                ),
                              ),
                            ),
                  ),
                  const _SidebarDivider(),
                  _SidebarAction(
                    icon: Icons.bookmark_border_rounded,
                    title: context.l10n.savedTitle,
                    subtitle: context.l10n.savedHint,
                    onTap: () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) => SavedScreen(appState: widget.appState),
                      ),
                    ),
                  ),
                  const _SidebarDivider(),
                  _SidebarAction(
                    icon: Icons.dns_outlined,
                    title: context.l10n.relaysTitle,
                    subtitle: _relayHostLabel(cfg?.publicBaseUrl),
                    onTap: () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) => RelaysScreen(appState: widget.appState),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        );
      },
    );
  }

  String _relayHostLabel(String? publicBaseUrl) {
    final raw = publicBaseUrl?.trim() ?? '';
    if (raw.isEmpty) return '';
    final parsed = Uri.tryParse(raw);
    return parsed?.host.isNotEmpty == true ? parsed!.host : raw;
  }

  void _openComposer(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => ComposeScreen(appState: widget.appState),
      ),
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
    final draft =
        await DraftStore.read(username: cfg.username, domain: cfg.domain);
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
      MaterialPageRoute(
          builder: (_) => ComposeScreen(appState: widget.appState)),
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

class _SidebarNotificationRow extends StatelessWidget {
  const _SidebarNotificationRow({required this.item, required this.lastSeenMs});

  final Map<String, dynamic> item;
  final int lastSeenMs;

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

    final when =
        ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts).toLocal() : null;
    final isNew = ts > 0 && ts > lastSeenMs;

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

    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Row(
        children: [
          if (isNew)
            Container(
              width: 8,
              height: 8,
              decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.primary,
                  borderRadius: BorderRadius.circular(99)),
            )
          else
            const SizedBox(width: 8),
          const SizedBox(width: 8),
          Icon(icon, size: 16),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              actor.isNotEmpty ? '$label · ${_shortActor(actor)}' : label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
            ),
          ),
          if (when != null)
            Text(
              formatTimeAgo(context, when),
              style: TextStyle(
                  fontSize: 11,
                  color:
                      Theme.of(context).colorScheme.onSurface.withAlpha(128)),
            ),
        ],
      ),
    );
  }

  static String _shortActor(String actor) {
    final uri = Uri.tryParse(actor);
    if (uri == null || uri.host.isEmpty) return actor;
    if (uri.pathSegments.isNotEmpty &&
        uri.pathSegments.first == 'users' &&
        uri.pathSegments.length >= 2) {
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
        if (inner == 'Follow' ||
            inner == 'Accept' ||
            inner == 'Reject' ||
            inner == 'Like' ||
            inner == 'EmojiReact') {
          return inner;
        }
      }
    }
    return type;
  }
}

class _SidebarAction extends StatelessWidget {
  const _SidebarAction({
    required this.title,
    required this.subtitle,
    this.icon,
    this.leading,
    this.onTap,
  });

  final String title;
  final String subtitle;
  final IconData? icon;
  final Widget? leading;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final mark = leading ??
        Container(
          width: 30,
          height: 30,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(10),
            gradient: LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: [
                scheme.primary.withAlpha(210),
                const Color(UiTokens.brandCyan).withAlpha(190),
              ],
            ),
          ),
          child: Icon(icon, size: 16, color: Colors.white),
        );
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: BorderRadius.circular(UiTokens.radiusCardSm),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
          child: Row(
            children: [
              mark,
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontWeight: FontWeight.w800,
                        fontSize: 13,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      subtitle,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 11,
                        color: scheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
              Icon(
                Icons.chevron_right_rounded,
                size: 18,
                color: scheme.onSurfaceVariant,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SidebarDivider extends StatelessWidget {
  const _SidebarDivider();

  @override
  Widget build(BuildContext context) {
    return Divider(
      height: 1,
      indent: 50,
      endIndent: 12,
      color: Theme.of(context).colorScheme.onSurface.withAlpha(18),
    );
  }
}

class _ComposeAvatar extends StatelessWidget {
  const _ComposeAvatar({required this.iconUrl, required this.size});

  final String iconUrl;
  final double size;

  @override
  Widget build(BuildContext context) {
    final fallback = Container(
      width: size,
      height: size,
      decoration: const BoxDecoration(
        shape: BoxShape.circle,
        gradient: LinearGradient(
          colors: [
            Color(UiTokens.brandViolet),
            Color(UiTokens.brandCyan),
          ],
        ),
      ),
      child: Icon(Icons.person_rounded, size: size * 0.55, color: Colors.white),
    );
    if (iconUrl.trim().isEmpty) return fallback;
    return ClipOval(
      child: Image.network(
        iconUrl,
        width: size,
        height: size,
        fit: BoxFit.cover,
        errorBuilder: (_, __, ___) => fallback,
      ),
    );
  }
}
