/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import '../../services/app_activity.dart';

import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/chat_models.dart';
import '../../model/core_config.dart';
import '../../services/actor_repository.dart';
import '../../services/core_event_stream.dart';
import '../../state/app_state.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/network_error_card.dart';
import '../theme/ui_tokens.dart';
import '../utils/time_ago.dart';
import 'chat_search_screen.dart';
import 'chat_thread_screen.dart';
import 'new_chat_screen.dart';

class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  bool _loading = false;
  Timer? _poll;
  StreamSubscription<bool>? _activity;
  String? _error;
  String? _next;
  List<ChatThreadItem> _threads = const [];
  final Map<String, ActorProfile> _peerProfiles = {};
  final Set<String> _peerProfilesInFlight = <String>{};
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  Timer? _loadRetry;
  CoreConfig? _streamConfig;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance
        .addPostFrameCallback((_) => _loadThreads(reset: true));
    _markSeen();
    _poll = AppActivity.periodic(
        const Duration(seconds: 10), (_) => _loadThreads(reset: true));
    _activity = AppActivity.changes.listen((foreground) {
      if (foreground && mounted) _loadThreads(reset: true);
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
        if (mounted && _threads.isEmpty) {
          _loadThreads(reset: true);
        }
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
    _activity?.cancel();
    _streamSub?.cancel();
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
    _loadRetry?.cancel();
    widget.appState.removeListener(_appStateListener);
    super.dispose();
  }

  Future<void> _markSeen() async {
    final now = DateTime.now().millisecondsSinceEpoch;
    await widget.appState
        .updatePrefs((current) => current.copyWith(lastChatSeenMs: now));
    widget.appState.clearUnreadChats();
  }

  void _startStream() {
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_streamConfig, cfg) && _streamSub != null) return;
    _streamConfig = cfg;
    _streamSub?.cancel();
    _streamSub = CoreEventStream(config: cfg).stream(kind: 'chat').listen((ev) {
      if (!mounted) return;
      if (ev.kind != 'chat') return;
      final ty = ev.activityType ?? '';
      if (ty.startsWith('typing:')) return;
      _streamDebounce?.cancel();
      _streamDebounce = Timer(const Duration(milliseconds: 400), () {
        if (!mounted) return;
        _loadThreads(reset: true);
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

  Future<void> _loadThreads({required bool reset}) async {
    if (!mounted || _loading) return;
    final cfg = widget.appState.config;
    if (cfg == null || !widget.appState.isRunning) return;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) _next = null;
    });
    try {
      final api = CoreApi(config: cfg);
      final resp =
          await api.fetchChatThreads(cursor: reset ? null : _next, limit: 50);
      final items = reset ? <ChatThreadItem>[] : List.of(_threads);
      final raw = resp['items'];
      if (raw is List) {
        for (final it in raw) {
          if (it is! Map) continue;
          items.add(ChatThreadItem.fromJson(it.cast<String, dynamic>()));
        }
      }
      final next = resp['next'];
      if (mounted) {
        setState(() {
          _threads = items;
          _next = next is String && next.trim().isNotEmpty ? next : null;
        });
        _updateUnreadCounts(items);
        unawaited(_resolvePeerProfiles(items));
      }
    } catch (e) {
      final msg = e.toString();
      if (mounted) setState(() => _error = msg);
      _scheduleRetryIfOffline(msg);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  /// Resolves the peer actor of every direct thread so the list can show the
  /// interlocutor's real avatar and nickname instead of the bare @handle.
  Future<void> _resolvePeerProfiles(List<ChatThreadItem> items) async {
    final targets = <String>{};
    for (final thread in items) {
      if (thread.kind != 'dm') continue;
      final actor = _peerActorForThread(thread);
      if (actor.isEmpty || _peerProfiles.containsKey(actor)) continue;
      if (_peerProfilesInFlight.contains(actor)) continue;
      targets.add(actor);
      if (targets.length >= 40) break;
    }
    if (targets.isEmpty) return;
    _peerProfilesInFlight.addAll(targets);
    try {
      final entries = await Future.wait(targets.map((actor) async {
        try {
          return MapEntry(
              actor, await ActorRepository.instance.getActor(actor));
        } catch (_) {
          return MapEntry<String, ActorProfile?>(actor, null);
        }
      }));
      if (!mounted) return;
      final found = <String, ActorProfile>{
        for (final entry in entries)
          if (entry.value != null) entry.key: entry.value!,
      };
      if (found.isEmpty) return;
      setState(() => _peerProfiles.addAll(found));
    } finally {
      _peerProfilesInFlight.removeAll(targets);
    }
  }

  /// Best-effort actor URL for a direct thread. Older cores only return the
  /// peer username, so fall back to the local actor URL in that case.
  String _peerActorForThread(ChatThreadItem thread) {
    if (thread.peerActors.isNotEmpty) return thread.peerActors.first.trim();
    if (thread.peerUsernames.isEmpty) return '';
    final username =
        thread.peerUsernames.first.trim().replaceFirst(RegExp(r'^@+'), '');
    if (username.isEmpty) return '';
    final cfg = widget.appState.config;
    final base = cfg?.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '') ?? '';
    if (base.isEmpty) return '';
    return '$base/users/${Uri.encodeComponent(username)}';
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
    if (_loadRetry != null && _loadRetry!.isActive) return;
    _loadRetry = Timer(const Duration(seconds: 1), () {
      if (!mounted) return;
      if (!widget.appState.isRunning) return;
      _loadThreads(reset: true);
    });
  }

  void _updateUnreadCounts(List<ChatThreadItem> items) {
    final seen = widget.appState.prefs.chatThreadSeenMs;
    var unread = 0;
    for (final t in items) {
      final lastMs = t.lastMessageMs;
      if (lastMs == null) continue;
      final seenMs = seen[t.threadId] ?? 0;
      if (lastMs > seenMs) unread += 1;
    }
    widget.appState.setUnreadChats(unread);
  }

  Future<void> _openNewChatDialog() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => NewChatScreen(appState: widget.appState),
      ),
    );
    if (mounted) await _loadThreads(reset: true);
  }

  Future<void> _openJoinDialog() async {
    final clip = await Clipboard.getData(Clipboard.kTextPlain);
    final controller = TextEditingController(text: clip?.text?.trim() ?? '');
    if (!mounted) {
      controller.dispose();
      return;
    }
    final raw = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.chatJoinTitle),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: InputDecoration(
            hintText: context.l10n.chatJoinHint,
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(UiTokens.radiusInput),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(controller.text),
            child: Text(context.l10n.chatJoinAction),
          ),
        ],
      ),
    );
    controller.dispose();
    if (raw == null || raw.trim().isEmpty) return;
    final match =
        RegExp(r'urn:fedi3:chat:[A-Za-z0-9:_-]+').firstMatch(raw.trim());
    final threadId = match?.group(0) ?? '';
    if (threadId.isEmpty) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.chatJoinInvalid)),
      );
      return;
    }
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      await CoreApi(config: cfg).joinChatThread(threadId: threadId);
      await _loadThreads(reset: true);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.chatJoinSuccess)),
      );
      await Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (context) => ChatThreadScreen(
            appState: widget.appState,
            threadId: threadId,
            title: context.l10n.chatGroup,
            isGroup: true,
          ),
        ),
      );
      if (!mounted) return;
      await _loadThreads(reset: true);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final title = context.l10n.chatTitle;
    return Scaffold(
      appBar: AppBar(
        title: Text(title),
        actions: [
          IconButton(
            tooltip: context.l10n.chatSearchAllTitle,
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (context) =>
                    ChatSearchScreen(appState: widget.appState),
              ),
            ),
            icon: const Icon(Icons.search_rounded),
          ),
          IconButton(
            tooltip: context.l10n.chatJoinTitle,
            onPressed: _openJoinDialog,
            icon: const Icon(Icons.group_add_outlined),
          ),
          IconButton(
            tooltip: context.l10n.chatRefresh,
            onPressed: _loading ? null : () => _loadThreads(reset: true),
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton(
        tooltip: context.l10n.chatNewTooltip,
        onPressed: _openNewChatDialog,
        child: const Icon(Icons.add_comment),
      ),
      body: RefreshIndicator(
        onRefresh: () async {
          await _loadThreads(reset: true);
          await _markSeen();
        },
        child: _buildBody(),
      ),
    );
  }

  Widget _buildBody() {
    if (_loading && _threads.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null && _threads.isEmpty) {
      return Padding(
        padding: const EdgeInsets.all(16),
        child: NetworkErrorCard(
          message: _error,
          onRetry: () => _loadThreads(reset: true),
        ),
      );
    }
    if (_threads.isEmpty) {
      return ListView(children: [
        const SizedBox(height: 120),
        EmptyState(
          icon: Icons.forum_outlined,
          title: context.l10n.chatThreadsEmpty,
          message: context.l10n.chatEmptyHint,
        )
      ]);
    }
    final pinnedIds = widget.appState.prefs.pinnedChatThreads.toSet();
    final pinned = <ChatThreadItem>[];
    final rest = <ChatThreadItem>[];
    for (final t in _threads) {
      if (pinnedIds.contains(t.threadId)) {
        pinned.add(t);
      } else {
        rest.add(t);
      }
    }
    final all = [...pinned, ...rest];
    return ListView.builder(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 96),
      itemCount: all.length + (_next != null ? 1 : 0),
      itemBuilder: (context, index) {
        if (index >= all.length) {
          return Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Center(
              child: OutlinedButton(
                onPressed: _loading ? null : () => _loadThreads(reset: false),
                child: Text(context.l10n.listLoadMore),
              ),
            ),
          );
        }
        final thread = all[index];
        final peer = thread.peerUsernames.isNotEmpty
            ? thread.peerUsernames.first.trim().replaceFirst(RegExp(r'^@+'), '')
            : '';
        final peerActor = _peerActorForThread(thread);
        final peerProfile = peerActor.isEmpty ? null : _peerProfiles[peerActor];
        final peerNickname = peerProfile?.displayName.trim() ?? '';
        final name = thread.kind == 'dm'
            ? (peerNickname.isNotEmpty
                ? peerNickname
                : (peer.isEmpty ? context.l10n.chatDirectMessage : peer))
            : (thread.title?.trim().isNotEmpty == true
                ? thread.title!.trim()
                : context.l10n.chatGroup);
        final preview = thread.lastMessagePreview?.trim().isNotEmpty == true
            ? thread.lastMessagePreview!.trim()
            : context.l10n.chatNoMessages;
        final ts = thread.lastMessageMs ?? thread.updatedAtMs;
        final when =
            ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts) : DateTime.now();
        final seenMs =
            widget.appState.prefs.chatThreadSeenMs[thread.threadId] ?? 0;
        final isUnread = thread.lastMessageMs != null && ts > seenMs;
        final isPinned = pinnedIds.contains(thread.threadId);
        return Padding(
          padding: const EdgeInsets.only(bottom: 10),
          child: GlassPanel(
            padding: const EdgeInsets.all(14),
            onLongPress: () => _showThreadMenu(thread, isPinned),
            onTap: () async {
              await _markThreadSeen(thread, ts);
              if (!context.mounted) return;
              await Navigator.of(context).push(
                MaterialPageRoute(
                  builder: (_) => ChatThreadScreen(
                    appState: widget.appState,
                    threadId: thread.threadId,
                    title: name,
                    isGroup: thread.kind == 'group',
                  ),
                ),
              );
              if (mounted) {
                await _loadThreads(reset: true);
              }
            },
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (thread.kind == 'dm')
                  _ChatListAvatar(
                    imageUrl: peerProfile?.iconUrl ?? '',
                    fallbackLabel: name,
                  )
                else
                  Container(
                    width: 46,
                    height: 46,
                    decoration: const BoxDecoration(
                      shape: BoxShape.circle,
                      gradient: LinearGradient(
                        begin: Alignment.topLeft,
                        end: Alignment.bottomRight,
                        colors: [
                          Color(UiTokens.brandViolet),
                          Color(UiTokens.brandCyan),
                        ],
                      ),
                    ),
                    alignment: Alignment.center,
                    child: Text(
                      name.characters.first.toUpperCase(),
                      style: const TextStyle(
                        color: Colors.white,
                        fontWeight: FontWeight.w800,
                        fontSize: 18,
                      ),
                    ),
                  ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          if (isPinned) ...[
                            const Icon(Icons.push_pin_rounded, size: 14),
                            const SizedBox(width: 5),
                          ],
                          Expanded(
                            child: Text(
                              name,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style:
                                  const TextStyle(fontWeight: FontWeight.w800),
                            ),
                          ),
                          const SizedBox(width: 8),
                          Text(
                            formatTimeAgo(context, when),
                            style: TextStyle(
                              fontSize: 11,
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurfaceVariant,
                            ),
                          ),
                          IconButton(
                            tooltip: context.l10n.chatMoreActions,
                            onPressed: () => _showThreadMenu(thread, isPinned),
                            visualDensity: VisualDensity.compact,
                            padding: EdgeInsets.zero,
                            constraints: const BoxConstraints(
                              minWidth: 28,
                              minHeight: 28,
                            ),
                            icon: Icon(
                              Icons.more_horiz_rounded,
                              size: 18,
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 4),
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Expanded(
                            child: Text(
                              preview,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 12.5,
                                height: 1.35,
                                color: isUnread
                                    ? Theme.of(context).colorScheme.onSurface
                                    : Theme.of(context)
                                        .colorScheme
                                        .onSurfaceVariant,
                                fontWeight: isUnread
                                    ? FontWeight.w600
                                    : FontWeight.w400,
                              ),
                            ),
                          ),
                          if (isUnread)
                            Padding(
                              padding: const EdgeInsets.only(left: 8, top: 4),
                              child: Container(
                                width: 9,
                                height: 9,
                                decoration: const BoxDecoration(
                                  shape: BoxShape.circle,
                                  gradient: LinearGradient(
                                    colors: [
                                      Color(UiTokens.brandViolet),
                                      Color(UiTokens.brandCyan),
                                    ],
                                  ),
                                  boxShadow: [
                                    BoxShadow(
                                      color: Color(0x667C6CF6),
                                      blurRadius: 8,
                                    ),
                                  ],
                                ),
                              ),
                            ),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  Future<void> _markThreadSeen(ChatThreadItem thread, int ts) async {
    final prefs = widget.appState.prefs;
    final now = DateTime.now().millisecondsSinceEpoch;
    final next = Map<String, int>.from(prefs.chatThreadSeenMs);
    next[thread.threadId] = ts > now ? ts + 1 : now;
    await widget.appState
        .updatePrefs((current) => current.copyWith(chatThreadSeenMs: next));
    if (mounted) setState(() {});
  }

  Future<void> _showThreadMenu(ChatThreadItem thread, bool pinned) async {
    final isGroup = thread.kind == 'group';
    final action = await showModalBottomSheet<String>(
      context: context,
      backgroundColor: Theme.of(context).colorScheme.surface,
      showDragHandle: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(
          top: Radius.circular(UiTokens.radiusCard),
        ),
      ),
      builder: (context) {
        return SafeArea(
          child: Wrap(
            children: [
              ListTile(
                leading:
                    Icon(pinned ? Icons.push_pin_outlined : Icons.push_pin),
                title: Text(
                    pinned ? context.l10n.chatUnpin : context.l10n.chatPin),
                onTap: () => Navigator.of(context).pop('pin'),
              ),
              ListTile(
                leading: Icon(isGroup
                    ? Icons.logout_rounded
                    : Icons.delete_outline_rounded),
                title: Text(isGroup
                    ? context.l10n.chatLeaveGroup
                    : context.l10n.chatDeleteThread),
                subtitle: Text(isGroup
                    ? context.l10n.chatLeaveGroupHint
                    : context.l10n.chatDeleteThreadHint),
                onTap: () => Navigator.of(context).pop(
                  isGroup ? 'leave' : 'delete',
                ),
              ),
            ],
          ),
        );
      },
    );
    if (action == null) return;
    if (action == 'pin') {
      await widget.appState.updatePrefs((current) {
        final pins = List<String>.from(current.pinnedChatThreads);
        if (pinned) {
          pins.removeWhere((id) => id == thread.threadId);
        } else if (!pins.contains(thread.threadId)) {
          pins.add(thread.threadId);
        }
        return current.copyWith(pinnedChatThreads: pins);
      });
      if (mounted) setState(() {});
      return;
    }

    final cfg = widget.appState.config;
    if (cfg == null || !mounted) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(action == 'leave'
            ? context.l10n.chatLeaveGroup
            : context.l10n.chatDeleteThread),
        content: Text(action == 'leave'
            ? context.l10n.chatLeaveGroupHint
            : context.l10n.chatDeleteThreadHint),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(action == 'leave'
                ? context.l10n.chatLeaveGroup
                : context.l10n.chatDeleteThread),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    try {
      final api = CoreApi(config: cfg);
      if (action == 'leave') {
        await api.leaveChatThread(threadId: thread.threadId);
      } else {
        await api.deleteChatThread(threadId: thread.threadId);
      }
      if (mounted) await _loadThreads(reset: true);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(e.toString())),
      );
    }
  }
}

class _ChatListAvatar extends StatelessWidget {
  const _ChatListAvatar({
    required this.imageUrl,
    required this.fallbackLabel,
  });

  final String imageUrl;
  final String fallbackLabel;

  @override
  Widget build(BuildContext context) {
    final url = imageUrl.trim();
    final fallback = _fallbackAvatar();
    if (url.isEmpty) return fallback;
    final dpr = MediaQuery.of(context).devicePixelRatio;
    return ClipOval(
      child: Image.network(
        url,
        width: 46,
        height: 46,
        fit: BoxFit.cover,
        cacheWidth: (46 * dpr).round(),
        filterQuality: FilterQuality.low,
        errorBuilder: (_, __, ___) => fallback,
      ),
    );
  }

  Widget _fallbackAvatar() {
    final label = fallbackLabel.trim();
    final initial = label.isEmpty ? '?' : label.characters.first.toUpperCase();
    return Container(
      width: 46,
      height: 46,
      decoration: const BoxDecoration(
        shape: BoxShape.circle,
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            Color(UiTokens.brandViolet),
            Color(UiTokens.brandCyan),
          ],
        ),
      ),
      alignment: Alignment.center,
      child: Text(
        initial,
        style: const TextStyle(
          color: Colors.white,
          fontWeight: FontWeight.w800,
          fontSize: 18,
        ),
      ),
    );
  }
}
