import '../../services/app_activity.dart';
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_selector/file_selector.dart';
import 'package:mime/mime.dart';
import 'package:audioplayers/audioplayers.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/chat_models.dart';
import '../../model/core_config.dart';
import '../../model/note_models.dart';
import '../../model/ui_prefs.dart';
import '../../services/actor_repository.dart';
import '../../services/core_event_stream.dart';
import '../../services/gif_service.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../utils/time_ago.dart';
import '../utils/open_url.dart';
import '../utils/media_url.dart';
import '../widgets/network_error_card.dart';
import '../widgets/inline_media_tile.dart';
import '../screens/media_viewer_screen.dart';
import 'call_screen.dart';
import '../widgets/emoji_picker.dart';
import '../widgets/audio_note_recorder.dart';
import '../widgets/media_note_recorder.dart';
import '../widgets/status_avatar.dart';
import '../../state/peer_presence_store.dart';
import '../../state/chat_status_store.dart';
import 'package:http/http.dart' as http;

class ChatThreadScreen extends StatefulWidget {
  const ChatThreadScreen({
    super.key,
    required this.appState,
    required this.threadId,
    required this.title,
    this.isGroup = false,
    this.initialSearch,
  });

  final AppState appState;
  final String threadId;
  final String title;
  final bool isGroup;
  final String? initialSearch;

  @override
  State<ChatThreadScreen> createState() => _ChatThreadScreenState();
}

class _ChatThreadScreenState extends State<ChatThreadScreen> {
  bool _loading = false;
  String? _error;
  String? _next;
  List<ChatMessageItem> _messages = const [];
  late String _title;
  final TextEditingController _composerCtrl = TextEditingController();
  late final FocusNode _composerFocus;
  final TextEditingController _searchCtrl = TextEditingController();
  bool _searchOpen = false;
  String _searchQuery = '';
  List<ChatMessageItem> _searchResults = const [];
  bool _searching = false;
  Timer? _searchDebounce;
  Map<String, dynamic>? _pin;
  String _myRole = 'member';
  Timer? _callPollTimer;
  int _lastCallSignalMs = 0;
  String? _incomingCallId;
  bool _callPromptOpen = false;
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  Timer? _loadRetry;
  Timer? _typingDebounce;
  final List<_PickedMedia> _media = [];
  bool _sending = false;
  Map<String, _StatusSummary> _statusMap = const {};
  final Set<String> _seenMessageIds = <String>{};
  Map<String, List<_ReactionItem>> _reactions = const {};
  ChatMessageItem? _replyTo;
  final Map<String, int> _typingActors = {};
  List<String> _recipients = const [];
  Map<String, ActorProfile> _profiles = const {};
  int _lastTypingSentMs = 0;
  CoreConfig? _streamConfig;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;

  @override
  void initState() {
    super.initState();
    _title = widget.title;
    _composerFocus = FocusNode(
      debugLabel: 'chat-composer',
      onKeyEvent: _handleComposerKey,
    );
    final initialSearch = widget.initialSearch?.trim() ?? '';
    if (initialSearch.isNotEmpty) {
      _searchOpen = true;
      _searchQuery = initialSearch;
      _searchCtrl.text = initialSearch;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _loadMessages(reset: true);
      _loadThreadMeta();
      if (initialSearch.isNotEmpty) {
        _runSearch(initialSearch);
      }
    });
    _markSeen();
    _markThreadSeen();
    _lastCallSignalMs = DateTime.now().millisecondsSinceEpoch - 30000;
    _callPollTimer = AppActivity.periodic(const Duration(seconds: 2), (_) {
      _pollCallSignals();
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
        if (mounted && _messages.isEmpty) {
          _loadMessages(reset: true);
        }
        if (mounted && _recipients.isEmpty) {
          _loadThreadMeta();
        }
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);
    if (widget.appState.isRunning) {
      _startStream();
    }
    _composerCtrl.addListener(_onComposerChanged);
  }

  @override
  void didUpdateWidget(covariant ChatThreadScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.title != oldWidget.title) {
      _title = widget.title;
      _composerFocus = FocusNode(
        debugLabel: 'chat-composer',
        onKeyEvent: _handleComposerKey,
      );
    }
  }

  @override
  void dispose() {
    _streamSub?.cancel();
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
    _loadRetry?.cancel();
    _typingDebounce?.cancel();
    _searchDebounce?.cancel();
    _callPollTimer?.cancel();
    _composerFocus.dispose();
    _composerCtrl.dispose();
    _searchCtrl.dispose();
    widget.appState.removeListener(_appStateListener);
    super.dispose();
  }

  Future<void> _markSeen() async {
    final now = DateTime.now().millisecondsSinceEpoch;
    await widget.appState
        .updatePrefs((current) => current.copyWith(lastChatSeenMs: now));
    widget.appState.clearUnreadChats();
  }

  Future<void> _markThreadSeen({int? latestMs}) async {
    final prefs = widget.appState.prefs;
    final now = DateTime.now().millisecondsSinceEpoch;
    final target = latestMs != null && latestMs >= now ? latestMs + 1 : now;
    final next = Map<String, int>.from(prefs.chatThreadSeenMs);
    next[widget.threadId] = target;
    await widget.appState
        .updatePrefs((current) => current.copyWith(chatThreadSeenMs: next));
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
      if ((ev.activityId ?? '') != widget.threadId) return;
      final ty = ev.activityType ?? '';
      if (ty.startsWith('typing:')) {
        final actor = ty.substring('typing:'.length).trim();
        if (actor.isNotEmpty) {
          _markTyping(actor);
        }
        return;
      }
      if (ty == 'call') {
        _pollCallSignals();
        return;
      }
      if (ty == 'react') {
        final ids = _messages
            .map((m) => m.messageId)
            .where((id) => id.isNotEmpty)
            .toList();
        if (ids.isNotEmpty) {
          _loadReactions(ids);
        }
        return;
      }
      _streamDebounce?.cancel();
      _streamDebounce = Timer(const Duration(milliseconds: 300), () {
        if (!mounted) return;
        _loadMessages(reset: true);
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

  void _markTyping(String actor) {
    final now = DateTime.now().millisecondsSinceEpoch;
    setState(() => _typingActors[actor] = now + 4000);
    Future.delayed(const Duration(seconds: 4), () {
      if (!mounted) return;
      final expires = _typingActors[actor];
      if (expires != null && expires <= DateTime.now().millisecondsSinceEpoch) {
        setState(() => _typingActors.remove(actor));
      }
    });
  }

  void _onComposerChanged() {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final text = _composerCtrl.text.trim();
    if (text.isEmpty) return;
    _typingDebounce?.cancel();
    _typingDebounce = Timer(const Duration(milliseconds: 600), () async {
      final now = DateTime.now().millisecondsSinceEpoch;
      if (now - _lastTypingSentMs < 2000) return;
      _lastTypingSentMs = now;
      try {
        await CoreApi(config: cfg).sendChatTyping(threadId: widget.threadId);
      } catch (_) {}
    });
  }

  Future<void> _loadThreadMeta() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      final api = CoreApi(config: cfg);
      final resp = await api.fetchChatMembers(threadId: widget.threadId);
      final raw = resp['items'];
      final members = <String>[];
      var myRole = 'member';
      if (raw is List) {
        for (final it in raw) {
          if (it is! Map) continue;
          final id = it['actor_id']?.toString().trim() ?? '';
          if (id == _selfActor()) {
            final role = it['role']?.toString().trim() ?? '';
            if (role.isNotEmpty) myRole = role;
          } else if (id.isNotEmpty) {
            members.add(id);
          }
        }
      }
      if (!mounted) return;
      setState(() {
        _recipients = members;
        _myRole = myRole;
      });
      await _loadPin();
      final profiles = <String, ActorProfile>{};
      for (final actor in members.take(12)) {
        try {
          final profile = await ActorRepository.instance.getActor(actor);
          if (profile != null) profiles[actor] = profile;
        } catch (_) {
          // Keep the actor id: the header and bubbles fall back to initials.
        }
      }
      if (!mounted) return;
      setState(() {
        _profiles = profiles;
        if (!widget.isGroup && members.isNotEmpty) {
          final peerUsername = profiles.isNotEmpty
              ? profiles.values.first.preferredUsername.trim()
              : '';
          final fallback = _usernameFromActor(members.first);
          final username = peerUsername.isNotEmpty ? peerUsername : fallback;
          final clean = username.replaceFirst(RegExp(r'^@+'), '');
          if (clean.isNotEmpty) _title = '@$clean';
        }
      });
    } catch (_) {
      // Best-effort: bubbles fall back to username initials.
    }
  }

  void _onSearchChanged(String value) {
    setState(() {
      _searchQuery = value;
      _searching = false;
    });
    _searchDebounce?.cancel();
    final q = value.trim();
    if (q.length < 2) {
      setState(() => _searchResults = const []);
      return;
    }
    _searchDebounce = Timer(const Duration(milliseconds: 300), () {
      _runSearch(q);
    });
  }

  Future<void> _runSearch(String query) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() => _searching = true);
    try {
      final resp = await CoreApi(config: cfg)
          .searchChatMessages(threadId: widget.threadId, query: query);
      final raw = resp['items'];
      final items = <ChatMessageItem>[];
      if (raw is List) {
        for (final m in raw.whereType<Map>()) {
          items.add(ChatMessageItem.fromJson(m.cast<String, dynamic>()));
        }
      }
      if (!mounted || query != _searchQuery.trim()) return;
      setState(() {
        _searchResults = items;
        _searching = false;
      });
    } catch (_) {
      if (!mounted) return;
      setState(() => _searching = false);
    }
  }

  Widget _buildPinBanner() {
    final pin = _pin;
    if (pin == null) return const SizedBox.shrink();
    final preview = pin['preview']?.toString().trim() ?? '';
    return Container(
      margin: const EdgeInsets.fromLTRB(12, 2, 12, 6),
      padding: const EdgeInsets.fromLTRB(12, 8, 8, 8),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHigh,
        borderRadius: BorderRadius.circular(UiTokens.radiusInput),
        border: Border.all(
          color: Theme.of(context).colorScheme.primary.withAlpha(60),
        ),
      ),
      child: Row(
        children: [
          Icon(Icons.push_pin_rounded,
              size: 18, color: Theme.of(context).colorScheme.primary),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  context.l10n.chatPinnedMessage,
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: Theme.of(context).colorScheme.primary,
                        fontWeight: FontWeight.w700,
                      ),
                ),
                Text(
                  preview.isEmpty ? context.l10n.chatPinned : preview,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
          if (_canPin)
            IconButton(
              visualDensity: VisualDensity.compact,
              tooltip: context.l10n.chatUnpinMessage,
              onPressed: _unpinCurrent,
              icon: const Icon(Icons.close_rounded, size: 18),
            ),
        ],
      ),
    );
  }

  Future<void> _startCall(String mediaType) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (context) => CallScreen(
          appState: widget.appState,
          threadId: widget.threadId,
          title: _title,
          mediaType: mediaType,
          isGroup: widget.isGroup,
        ),
      ),
    );
    _incomingCallId = null;
  }

  Future<void> _pollCallSignals() async {
    final cfg = widget.appState.config;
    if (cfg == null || _callPromptOpen) return;
    try {
      final items = await CoreApi(config: cfg).fetchChatCallSignals(
        threadId: widget.threadId,
        afterMs: _lastCallSignalMs,
      );
      for (final s in items) {
        if (s.createdAtMs > _lastCallSignalMs) {
          _lastCallSignalMs = s.createdAtMs;
        }
        if (s.action != 'invite') continue;
        if (_incomingCallId == s.callId) continue;
        if (s.fromActor == _selfActor()) continue;
        final media = s.data['media']?.toString() ?? 'audio';
        _incomingCallId = s.callId;
        await _showIncomingCall(s, media);
        break;
      }
    } catch (_) {}
  }

  Future<void> _showIncomingCall(ChatCallSignal signal, String media) async {
    if (_callPromptOpen || !mounted) return;
    _callPromptOpen = true;
    final from = signal.fromActor;
    final accepted = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.callIncoming),
        content: Row(
          children: [
            CircleAvatar(
              radius: 24,
              backgroundColor:
                  Theme.of(context).colorScheme.primary.withAlpha(35),
              child: Icon(
                media == 'video' ? Icons.videocam_rounded : Icons.call_rounded,
                color: Theme.of(context).colorScheme.primary,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(_usernameFromActor(from)),
                  Text(
                    media == 'video'
                        ? context.l10n.callIncomingVideo
                        : context.l10n.callIncomingAudio,
                    style: Theme.of(context).textTheme.labelSmall,
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(context.l10n.callDecline),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.callAccept),
          ),
        ],
      ),
    );
    _callPromptOpen = false;
    if (accepted != true) {
      _incomingCallId = null;
      try {
        await CoreApi(config: widget.appState.config!).sendChatCallSignal(
          threadId: widget.threadId,
          callId: signal.callId,
          action: 'decline',
          to: from,
        );
      } catch (_) {}
      return;
    }
    if (!mounted) return;
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (context) => CallScreen(
          appState: widget.appState,
          threadId: widget.threadId,
          title: _title,
          mediaType: media,
          isGroup: widget.isGroup,
          incoming: true,
          callId: signal.callId,
          remoteActor: from,
        ),
      ),
    );
    _incomingCallId = null;
  }

  Future<void> _loadPin() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      final pin =
          await CoreApi(config: cfg).fetchChatPin(threadId: widget.threadId);
      if (!mounted) return;
      setState(() => _pin = pin);
    } catch (_) {
      // Best-effort: the banner is optional.
    }
  }

  bool get _canPin =>
      !widget.isGroup || _myRole == 'owner' || _myRole == 'admin';

  Future<void> _togglePin(ChatMessageItem message) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final pinned = _pin?['message_id'] == message.messageId;
    try {
      await CoreApi(config: cfg).setChatPin(
        threadId: widget.threadId,
        messageId: message.messageId,
        pinned: !pinned,
      );
      await _loadPin();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            pinned ? context.l10n.chatUnpinned : context.l10n.chatPinned,
          ),
        ),
      );
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _unpinCurrent() async {
    final cfg = widget.appState.config;
    final messageId = _pin?['message_id']?.toString();
    if (cfg == null || messageId == null || messageId.isEmpty) return;
    try {
      await CoreApi(config: cfg).setChatPin(
          threadId: widget.threadId, messageId: messageId, pinned: false);
      await _loadPin();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.chatUnpinned)),
      );
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  void _copyInviteLink() {
    Clipboard.setData(ClipboardData(text: 'fedi3://chat/${widget.threadId}'));
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(context.l10n.chatInviteCopied)),
    );
  }

  String _selfActor() {
    final cfg = widget.appState.config;
    if (cfg == null) return '';
    return '${cfg.publicBaseUrl.replaceAll(RegExp(r"/+$"), "")}/users/${cfg.username}';
  }

  String _usernameFromActor(String actorUrl) {
    final uri = Uri.tryParse(actorUrl.trim());
    if (uri == null) return actorUrl;
    for (final seg in uri.pathSegments.reversed) {
      final value = seg.trim();
      if (value.isEmpty) continue;
      return value.startsWith('@') ? value.substring(1) : value;
    }
    return actorUrl;
  }

  String _resolveMediaUrl(String url) {
    return resolveLocalMediaUrl(widget.appState.config, url);
  }

  Future<void> _loadMessages({required bool reset}) async {
    final cfg = widget.appState.config;
    if (cfg == null || !widget.appState.isRunning) return;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) _next = null;
    });
    try {
      final api = CoreApi(config: cfg);
      final resp = await api.fetchChatMessages(
        threadId: widget.threadId,
        cursor: reset ? null : _next,
        limit: 60,
      );
      final items = reset ? <ChatMessageItem>[] : List.of(_messages);
      final raw = resp['items'];
      if (raw is List) {
        for (final it in raw) {
          if (it is! Map) continue;
          items.add(ChatMessageItem.fromJson(it.cast<String, dynamic>()));
        }
      }
      String? titleFromSystem;
      for (final msg in items) {
        final payload = msg.payload;
        if (payload == null) continue;
        if (payload.op == 'system' && payload.action == 'rename') {
          final nextTitle = payload.title?.trim() ?? '';
          if (nextTitle.isNotEmpty) {
            titleFromSystem = nextTitle;
            break;
          }
        }
      }
      final next = resp['next'];
      if (mounted) {
        setState(() {
          _messages = items;
          _next = next is String && next.trim().isNotEmpty ? next : null;
          if (titleFromSystem != null && titleFromSystem != _title) {
            _title = titleFromSystem;
          }
        });
        if (_recipients.isEmpty) {
          unawaited(_loadThreadMeta());
        }
      }
      if (items.isNotEmpty) {
        final selfActor = _selfActor();
        final pendingSeen = items
            .where((m) =>
                m.messageId.isNotEmpty &&
                m.senderActor != selfActor &&
                !_seenMessageIds.contains(m.messageId))
            .toList();
        if (pendingSeen.isNotEmpty) {
          await Future.wait(pendingSeen.map((m) async {
            _seenMessageIds.add(m.messageId);
            try {
              await api.markChatSeen(
                  threadId: m.threadId, messageId: m.messageId);
            } catch (_) {
              // Retry on the next load if the receipt could not be sent.
              _seenMessageIds.remove(m.messageId);
            }
          }));
        }
        await _markSeen();
        await _markThreadSeen(latestMs: items.first.createdAtMs);
      }
      final ids =
          items.map((m) => m.messageId).where((id) => id.isNotEmpty).toList();
      if (ids.isNotEmpty) {
        await _loadStatuses(ids);
        await _loadReactions(ids);
      }
    } catch (e) {
      final msg = e.toString();
      if (mounted) setState(() => _error = msg);
      _scheduleRetryIfOffline(msg);
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
        lower.contains('errno=111');
    if (!shouldRetry) return;
    if (_loadRetry != null && _loadRetry!.isActive) return;
    _loadRetry = Timer(const Duration(seconds: 1), () {
      if (!mounted) return;
      if (!widget.appState.isRunning) return;
      _loadMessages(reset: true);
    });
  }

  Future<void> _loadStatuses(List<String> messageIds) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      final api = CoreApi(config: cfg);
      final resp = await api.fetchChatStatuses(messageIds: messageIds);
      final raw = resp['items'];
      if (raw is! List) return;
      final fresh = <String, _StatusSummary>{};
      for (final it in raw) {
        if (it is! Map) continue;
        final msgId = it['message_id']?.toString() ?? '';
        final status = it['status']?.toString() ?? '';
        if (msgId.isEmpty || status.isEmpty) continue;
        final current = fresh[msgId] ?? _StatusSummary();
        current.add(status);
        fresh[msgId] = current;
      }
      final cached = await ChatStatusStore.readAll();
      if (mounted) {
        // Keep the last known status for messages that are not returned by a
        // partial/failed status query, instead of showing them as pending.
        final merged = Map<String, _StatusSummary>.from(_statusMap);
        for (final entry in fresh.entries) {
          merged[entry.key] = entry.value;
        }
        for (final entry in cached.entries) {
          final current = merged[entry.key];
          final currentRank =
              current == null ? -1 : ChatStatusStore.rank(current.bestStatus);
          if (ChatStatusStore.rank(entry.value) > currentRank) {
            final summary = _StatusSummary()..add(entry.value);
            merged[entry.key] = summary;
          }
        }
        setState(() => _statusMap = merged);
      }
      if (fresh.isNotEmpty) {
        unawaited(ChatStatusStore.merge({
          for (final entry in fresh.entries) entry.key: entry.value.bestStatus,
        }));
      }
    } catch (_) {
      // best-effort
    }
  }

  Future<void> _loadReactions(List<String> messageIds) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      final api = CoreApi(config: cfg);
      final resp = await api.fetchChatReactions(messageIds: messageIds);
      final raw = resp['items'];
      if (raw is! List) return;
      final map = <String, List<_ReactionItem>>{};
      for (final it in raw) {
        if (it is! Map) continue;
        final msgId = it['message_id']?.toString() ?? '';
        if (msgId.isEmpty) continue;
        final reactions = <_ReactionItem>[];
        final list = it['reactions'];
        if (list is List) {
          for (final r in list) {
            if (r is! Map) continue;
            final reaction = r['reaction']?.toString() ?? '';
            if (reaction.isEmpty) continue;
            final count = (r['count'] as num?)?.toInt() ?? 0;
            final me = r['me'] == true;
            reactions
                .add(_ReactionItem(reaction: reaction, count: count, me: me));
          }
        }
        map[msgId] = reactions;
      }
      if (mounted) setState(() => _reactions = map);
    } catch (_) {
      // best-effort
    }
  }

  Future<void> _openReactionPicker(ChatMessageItem message) async {
    final prefs = widget.appState.prefs;
    final emoji = await EmojiPicker.show(context, prefs: prefs);
    if (emoji == null || emoji.isEmpty) return;
    await _sendReaction(message, emoji, remove: false);
  }

  Future<void> _sendReaction(ChatMessageItem message, String reaction,
      {required bool remove}) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      await CoreApi(config: cfg).sendChatReaction(
        messageId: message.messageId,
        reaction: reaction,
        remove: remove,
      );
      final ids = _messages
          .map((m) => m.messageId)
          .where((id) => id.isNotEmpty)
          .toList();
      if (ids.isNotEmpty) {
        await _loadReactions(ids);
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _sendMessage() async {
    if (_sending) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final draft = _composerCtrl.text;
    final text = draft.trim();
    if (text.isEmpty && _media.isEmpty) return;
    final media = List.of(_media);
    final reply = _replyTo;
    _focusComposer();
    setState(() => _sending = true);
    try {
      final api = CoreApi(config: cfg);
      final attachments = <Map<String, dynamic>>[];
      for (final m in media) {
        if (m.coreMediaId == null || m.url == null || m.mediaType == null) {
          final resp = await api.uploadMedia(bytes: m.bytes, filename: m.name);
          final id = (resp['id'] as String?)?.trim();
          final url = (resp['url'] as String?)?.trim();
          final mediaType = (resp['media_type'] as String?)?.trim() ??
              lookupMimeType(m.name) ??
              '';
          if (id != null && id.isNotEmpty && url != null && url.isNotEmpty) {
            m.coreMediaId = id;
            m.url = url;
            m.mediaType = mediaType;
          }
        }
        if (m.coreMediaId != null && m.url != null && m.mediaType != null) {
          attachments.add({
            'id': m.coreMediaId,
            'url': m.url,
            'mediaType': m.mediaType,
            'name': m.name,
          });
        }
      }
      await api.sendChatMessage(
        threadId: widget.threadId,
        recipients: const [],
        text: text,
        attachments: attachments,
        replyTo: reply?.messageId,
      );
      if (!mounted) return;
      if (_composerCtrl.text == draft) _composerCtrl.clear();
      if (identical(_replyTo, reply)) _replyTo = null;
      _media.removeWhere(media.contains);
      await _loadMessages(reset: true);
    } catch (e) {
      if (mounted) {
        // The draft and the attachments stay so the send can be retried; the
        // error must be visible even when the thread already has messages.
        setState(() => _error = e.toString());
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(context.l10n.chatSendFailed(e.toString())),
            action: SnackBarAction(
              label: context.l10n.retry,
              onPressed: _sendMessage,
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _recordNote(bool video) async {
    if (video && Platform.isLinux) {
      await _pickVideoNote();
      return;
    }
    final path = await showDialog<String>(
      context: context,
      barrierDismissible: false,
      builder: (context) => video
          ? const MediaNoteRecorder(video: true)
          : const AudioNoteRecorder(),
    );
    if (path == null || path.trim().isEmpty) return;
    try {
      final bytes = await File(path).readAsBytes();
      if (!mounted) return;
      setState(() {
        _media.add(_PickedMedia(
          name: path.split(Platform.pathSeparator).last,
          bytes: bytes,
        ));
      });
      await _sendMessage();
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _pickVideoNote() async {
    try {
      final file = await openFile(
        acceptedTypeGroups: const [
          XTypeGroup(
            label: 'video',
            extensions: ['mp4', 'mov', 'webm', 'mkv', 'm4v'],
          ),
        ],
      );
      if (file == null) return;
      final bytes = await file.readAsBytes();
      if (!mounted) return;
      setState(() {
        _media.add(_PickedMedia(name: file.name, bytes: bytes));
      });
      await _sendMessage();
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _pickFiles() async {
    try {
      final files = await openFiles();
      for (final f in files) {
        final bytes = await f.readAsBytes();
        final name = f.name.isNotEmpty ? f.name : 'file';
        _media.add(_PickedMedia(name: name, bytes: bytes));
      }
      if (mounted) setState(() {});
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _insertEmoji() async {
    final prefs = widget.appState.prefs;
    final emoji = await EmojiPicker.show(context, prefs: prefs);
    if (emoji == null || emoji.isEmpty) return;
    final text = _composerCtrl.text;
    final sel = _composerCtrl.selection;
    final insertAt = sel.baseOffset >= 0 ? sel.baseOffset : text.length;
    final next = text.replaceRange(insertAt, insertAt, emoji);
    _composerCtrl.text = next;
    _composerCtrl.selection =
        TextSelection.collapsed(offset: insertAt + emoji.length);
  }

  Future<void> _openRenameDialog() async {
    if (!widget.isGroup) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final ctrl = TextEditingController(text: _title);
    final result = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.chatRename),
        content: TextField(
          controller: ctrl,
          decoration: InputDecoration(hintText: context.l10n.chatRenameHint),
        ),
        actions: [
          TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: Text(context.l10n.cancel)),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(ctrl.text.trim()),
            child: Text(context.l10n.chatSave),
          ),
        ],
      ),
    );
    ctrl.dispose();
    if (result == null || result.isEmpty) return;
    try {
      await CoreApi(config: cfg)
          .updateChatThreadTitle(threadId: widget.threadId, title: result);
      if (mounted) {
        setState(() => _title = result);
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _openMembersDialog() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    final searchCtrl = TextEditingController();
    List<String> members = const [];
    final Map<String, ActorProfile> memberProfiles = {};
    final Map<String, String> roles = {};
    List<ActorProfile> suggestions = const [];
    bool loading = true;
    bool working = false;
    String? error;
    Timer? debounce;
    final selfActor =
        '${cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '')}/users/${cfg.username}';

    Future<void> loadMembers() async {
      try {
        final resp = await api.fetchChatMembers(threadId: widget.threadId);
        final raw = resp['items'];
        final list = <String>[];
        if (raw is List) {
          for (final it in raw) {
            if (it is! Map) continue;
            final id = it['actor_id']?.toString() ?? '';
            final role = it['role']?.toString() ?? 'member';
            if (id.isNotEmpty) {
              list.add(id);
              roles[id] = role;
            }
          }
        }
        members = list;
        for (final id in list) {
          try {
            final profile = await ActorRepository.instance.getActor(id);
            if (profile != null) memberProfiles[id] = profile;
          } catch (_) {
            // Keep the actor id if its profile cannot be fetched.
          }
        }
      } catch (e) {
        final msg = e.toString();
        if (!msg.contains('404')) {
          error = msg;
        }
      } finally {
        loading = false;
      }
    }

    await loadMembers();
    if (!mounted) return;

    await showDialog<void>(
      context: context,
      builder: (context) {
        return StatefulBuilder(
          builder: (context, setState) {
            void runSearch(String raw) {
              debounce?.cancel();
              debounce = Timer(const Duration(milliseconds: 250), () async {
                if (!context.mounted) return;
                final query = raw.trim();
                if (query.length < 2) {
                  setState(() => suggestions = const []);
                  return;
                }
                try {
                  final resp = await api.searchUsers(
                    query: query,
                    source: 'all',
                    consistency: 'best',
                    limit: 6,
                  );
                  final rawItems = resp['items'];
                  final list = <ActorProfile>[];
                  if (rawItems is List) {
                    for (final it in rawItems) {
                      if (it is! Map) continue;
                      final profile =
                          ActorProfile.tryParse(it.cast<String, dynamic>());
                      if (profile != null) list.add(profile);
                    }
                  }
                  if (context.mounted) setState(() => suggestions = list);
                } catch (_) {
                  if (context.mounted) setState(() => suggestions = const []);
                }
              });
            }

            Future<void> addMember(String actorInput) async {
              final value = actorInput.trim();
              if (value.isEmpty) return;
              if (!context.mounted) return;
              setState(() => working = true);
              try {
                final actor = await api.resolveActorInput(value);
                await api.updateChatThreadMembers(
                    threadId: widget.threadId, add: [actor]);
                if (!members.contains(actor)) {
                  members = [...members, actor];
                }
                roles[actor] = 'member';
                final profile = await ActorRepository.instance.getActor(actor);
                if (profile != null) memberProfiles[actor] = profile;
                searchCtrl.clear();
                suggestions = const [];
                if (context.mounted) setState(() {});
              } catch (e) {
                if (context.mounted) setState(() => error = e.toString());
              } finally {
                if (context.mounted) setState(() => working = false);
              }
            }

            Future<void> removeMember(String actor) async {
              if (!context.mounted) return;
              setState(() => working = true);
              try {
                await api.updateChatThreadMembers(
                    threadId: widget.threadId, remove: [actor]);
                members = members.where((m) => m != actor).toList();
                roles.remove(actor);
                memberProfiles.remove(actor);
                if (context.mounted) setState(() {});
              } catch (e) {
                if (context.mounted) setState(() => error = e.toString());
              } finally {
                if (context.mounted) setState(() => working = false);
              }
            }

            Future<void> setRole(String actor, String role) async {
              if (!context.mounted) return;
              setState(() => working = true);
              try {
                await api.setChatMemberRole(
                  threadId: widget.threadId,
                  actor: actor,
                  role: role,
                );
                roles[actor] = role;
                if (context.mounted) setState(() {});
              } catch (e) {
                if (context.mounted) setState(() => error = e.toString());
              } finally {
                if (context.mounted) setState(() => working = false);
              }
            }

            return AlertDialog(
              title: Text(context.l10n.chatMembers),
              content: SizedBox(
                width: 420,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (loading) const LinearProgressIndicator(minHeight: 2),
                    TextField(
                      controller: searchCtrl,
                      decoration: InputDecoration(
                        labelText: context.l10n.chatAddMember,
                        hintText: context.l10n.chatAddMemberHint,
                      ),
                      onChanged: runSearch,
                      onSubmitted: (v) => addMember(v),
                    ),
                    if (suggestions.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(top: 8),
                        child: Container(
                          constraints: const BoxConstraints(maxHeight: 160),
                          decoration: BoxDecoration(
                            borderRadius: BorderRadius.circular(12),
                            color: Theme.of(context)
                                .colorScheme
                                .surfaceContainerHighest,
                          ),
                          child: ListView.separated(
                            shrinkWrap: true,
                            itemCount: suggestions.length,
                            separatorBuilder: (_, __) =>
                                const Divider(height: 1),
                            itemBuilder: (context, index) {
                              final profile = suggestions[index];
                              return ListTile(
                                dense: true,
                                title: Text(profile.displayName,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis),
                                subtitle: Text(profile.id,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis),
                                onTap: () => addMember(profile.id),
                              );
                            },
                          ),
                        ),
                      ),
                    const SizedBox(height: 12),
                    if (error != null)
                      Text(error!,
                          style: TextStyle(
                              color: Theme.of(context).colorScheme.error)),
                    const SizedBox(height: 8),
                    SizedBox(
                      height: 260,
                      child: ListView.builder(
                        itemCount: members.length,
                        itemBuilder: (context, index) {
                          final actor = members[index];
                          final profile = memberProfiles[actor];
                          final role = roles[actor] ?? 'member';
                          final isOwner = role == 'owner';
                          final canManage =
                              roles[selfActor] == 'owner' && actor != selfActor;
                          return ListTile(
                            dense: true,
                            leading: StatusAvatar(
                              imageUrl: profile?.iconUrl ?? '',
                              size: 34,
                              showStatus: true,
                              statusKey: profile?.preferredUsername ?? '',
                            ),
                            title: Text(
                              profile == null
                                  ? _usernameFromActor(actor)
                                  : (profile.displayName.trim().isEmpty
                                      ? profile.preferredUsername
                                      : profile.displayName),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            subtitle: Text(
                              () {
                                final host = Uri.tryParse(actor)?.host ?? '';
                                final user = profile?.preferredUsername ??
                                    _usernameFromActor(actor);
                                return host.isEmpty ? '@$user' : '@$user@$host';
                              }(),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Container(
                                  padding: const EdgeInsets.symmetric(
                                      horizontal: 6, vertical: 2),
                                  decoration: BoxDecoration(
                                    color: Theme.of(context)
                                        .colorScheme
                                        .surfaceContainerHighest,
                                    borderRadius: BorderRadius.circular(6),
                                  ),
                                  child: Text(
                                    role == 'owner'
                                        ? context.l10n.chatRoleOwner
                                        : role == 'admin'
                                            ? context.l10n.chatRoleAdmin
                                            : context.l10n.chatRoleMember,
                                    style: const TextStyle(fontSize: 10),
                                  ),
                                ),
                                if (canManage)
                                  PopupMenuButton<String>(
                                    enabled: !working,
                                    tooltip: context.l10n.chatRoleChange,
                                    onSelected: (r) => setRole(actor, r),
                                    itemBuilder: (context) => [
                                      PopupMenuItem(
                                        value: 'admin',
                                        child: Text(context.l10n.chatRoleAdmin),
                                      ),
                                      PopupMenuItem(
                                        value: 'member',
                                        child:
                                            Text(context.l10n.chatRoleMember),
                                      ),
                                    ],
                                    child: const Padding(
                                      padding:
                                          EdgeInsets.symmetric(horizontal: 4),
                                      child: Icon(Icons.more_vert, size: 18),
                                    ),
                                  ),
                                if (!isOwner)
                                  IconButton(
                                    tooltip: context.l10n.chatRemoveMember,
                                    onPressed: working
                                        ? null
                                        : () => removeMember(actor),
                                    icon: const Icon(Icons.person_remove,
                                        size: 18),
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
              actions: [
                TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: Text(context.l10n.cancel)),
              ],
            );
          },
        );
      },
    );
    searchCtrl.dispose();
    debounce?.cancel();
  }

  Future<void> _openGifPicker() async {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (context) {
        return _GifPicker(
          prefs: widget.appState.prefs,
          onPick: (gif) async {
            Navigator.of(context).pop();
            try {
              final resp = await http.get(Uri.parse(gif.originalUrl));
              if (resp.statusCode < 200 || resp.statusCode >= 300) {
                throw StateError('gif download failed');
              }
              final bytes = resp.bodyBytes;
              final name = 'gif-${gif.id}.gif';
              setState(
                  () => _media.add(_PickedMedia(name: name, bytes: bytes)));
            } catch (e) {
              if (mounted) setState(() => _error = e.toString());
            }
          },
        );
      },
    );
  }

  Future<void> _forwardMessage(ChatMessageItem message) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    List<ChatThreadItem> threads = const [];
    try {
      final resp = await api.fetchChatThreads(limit: 100);
      final raw = resp['items'];
      if (raw is List) {
        threads = raw
            .whereType<Map>()
            .map((m) => ChatThreadItem.fromJson(m.cast<String, dynamic>()))
            .where((t) => t.threadId != widget.threadId)
            .toList();
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
      return;
    }
    if (!mounted) return;
    final target = await showDialog<ChatThreadItem>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.chatForwardTitle),
        content: SizedBox(
          width: 380,
          height: 360,
          child: threads.isEmpty
              ? Center(child: Text(context.l10n.chatEmpty))
              : ListView.builder(
                  itemCount: threads.length,
                  itemBuilder: (context, index) {
                    final t = threads[index];
                    final title = t.title?.trim().isNotEmpty == true
                        ? t.title!.trim()
                        : (t.kind == 'dm'
                            ? t.peerUsernames.join(', ')
                            : context.l10n.chatGroup);
                    return ListTile(
                      leading: Icon(t.kind == 'dm'
                          ? Icons.person_outline_rounded
                          : Icons.group_outlined),
                      title: Text(
                        title.isEmpty ? t.threadId : title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      onTap: () => Navigator.of(context).pop(t),
                    );
                  },
                ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: Text(context.l10n.cancel),
          ),
        ],
      ),
    );
    if (target == null) return;
    if (!mounted) return;
    final payload = message.payload;
    final attachments = <Map<String, dynamic>>[];
    for (final a in payload?.attachments ?? const <ChatAttachment>[]) {
      attachments.add({
        'id': a.id,
        'url': a.url,
        'media_type': a.mediaType,
        if (a.name != null && a.name!.trim().isNotEmpty) 'name': a.name,
      });
    }
    final text = _messageText(message).trim().isNotEmpty
        ? _messageText(message).trim()
        : context.l10n.chatMediaFallback;
    try {
      await api.sendChatMessage(
        threadId: target.threadId,
        recipients: const [],
        text: text,
        attachments: attachments,
      );
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.chatForwarded)),
      );
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _editMessage(ChatMessageItem message) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final payload = message.payload;
    final ctrl = TextEditingController(text: payload?.text ?? '');
    final result = await showDialog<String>(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: Text(context.l10n.chatEditTitle),
          content: TextField(
            controller: ctrl,
            minLines: 2,
            maxLines: 6,
            decoration: InputDecoration(hintText: context.l10n.chatMessageHint),
          ),
          actions: [
            TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: Text(context.l10n.cancel)),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(ctrl.text.trim()),
              child: Text(context.l10n.chatSave),
            ),
          ],
        );
      },
    );
    ctrl.dispose();
    if (result == null || result.trim().isEmpty) return;
    try {
      await CoreApi(config: cfg)
          .editChatMessage(messageId: message.messageId, text: result.trim());
    } on CoreApiException catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(e.message)));
    }
    await _loadMessages(reset: true);
  }

  Future<void> _deleteMessage(ChatMessageItem message) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.chatDeleteTitle),
        content: Text(context.l10n.chatDeleteHint),
        actions: [
          TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: Text(context.l10n.cancel)),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.chatDelete),
          ),
        ],
      ),
    );
    if (ok != true) return;
    try {
      await CoreApi(config: cfg).deleteChatMessage(messageId: message.messageId);
    } on CoreApiException catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(e.message)),
      );
    }
    await _loadMessages(reset: true);
  }

  Future<void> _deleteChat() => _removeChat(
        title: context.l10n.chatDeleteThread,
        hint: context.l10n.chatDeleteThreadHint,
        actionLabel: context.l10n.chatDeleteThread,
        leave: false,
      );

  Future<void> _deleteGroup() => _removeChat(
        title: context.l10n.chatDeleteGroup,
        hint: context.l10n.chatDeleteGroupHint,
        actionLabel: context.l10n.chatDeleteGroup,
        leave: false,
      );

  Future<void> _leaveChat() => _removeChat(
        title: context.l10n.chatLeaveGroup,
        hint: context.l10n.chatLeaveGroupHint,
        actionLabel: context.l10n.chatLeaveGroup,
        leave: true,
      );

  Future<void> _removeChat({
    required String title,
    required String hint,
    required String actionLabel,
    required bool leave,
  }) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(title),
        content: Text(hint),
        actions: [
          TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: Text(context.l10n.cancel)),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(actionLabel),
          ),
        ],
      ),
    );
    if (ok != true) return;
    final api = CoreApi(config: cfg);
    if (leave) {
      await api.leaveChatThread(threadId: widget.threadId);
    } else {
      await api.deleteChatThread(threadId: widget.threadId);
    }
    if (mounted) {
      Navigator.of(context).pop();
    }
  }

  String _presenceLabel(BuildContext context, PresenceStatus status) {
    return switch (status) {
      PresenceStatus.online => context.l10n.presenceOnline,
      PresenceStatus.busy => context.l10n.presenceBusy,
      PresenceStatus.afk => context.l10n.presenceAfk,
      PresenceStatus.offline => context.l10n.presenceOffline,
    };
  }

  Color _presenceColor(PresenceStatus status) {
    return switch (status) {
      PresenceStatus.online => const Color(0xFF3DDC97),
      PresenceStatus.busy => const Color(0xFFFF5A6E),
      PresenceStatus.afk => const Color(0xFFF2A33A),
      PresenceStatus.offline => const Color(0xFF8B93A5),
    };
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Row(
          children: [
            if (_recipients.length == 1) ...[
              StatusAvatar(
                imageUrl: _profiles[_recipients.first]?.iconUrl ?? '',
                size: 34,
                showStatus: true,
                statusKey: _profiles[_recipients.first]?.preferredUsername ??
                    _usernameFromActor(_recipients.first),
              ),
              const SizedBox(width: 10),
            ],
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(_title, maxLines: 1, overflow: TextOverflow.ellipsis),
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (!widget.isGroup && _recipients.length == 1)
                        ValueListenableBuilder<Map<String, PresenceStatus>>(
                          valueListenable:
                              PeerPresenceStore.instance.statusByUsername,
                          builder: (context, map, _) {
                            final username = _profiles[_recipients.first]
                                    ?.preferredUsername ??
                                _usernameFromActor(_recipients.first);
                            final status =
                                map[username] ?? PresenceStatus.offline;
                            return Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Container(
                                  width: 7,
                                  height: 7,
                                  decoration: BoxDecoration(
                                    shape: BoxShape.circle,
                                    color: _presenceColor(status),
                                  ),
                                ),
                                const SizedBox(width: 4),
                                Text(
                                  _presenceLabel(context, status),
                                  style: Theme.of(context)
                                      .textTheme
                                      .labelSmall
                                      ?.copyWith(
                                        color: Theme.of(context)
                                            .colorScheme
                                            .onSurfaceVariant,
                                      ),
                                ),
                                const SizedBox(width: 6),
                                Text('·',
                                    style: Theme.of(context)
                                        .textTheme
                                        .labelSmall
                                        ?.copyWith(
                                          color: Theme.of(context)
                                              .colorScheme
                                              .onSurfaceVariant,
                                        )),
                                const SizedBox(width: 6),
                              ],
                            );
                          },
                        )
                      else if (widget.isGroup || _recipients.isNotEmpty)
                        Text(
                          '${context.l10n.chatMembers} ${_recipients.length + 1} · ',
                          style:
                              Theme.of(context).textTheme.labelSmall?.copyWith(
                                    color: Theme.of(context)
                                        .colorScheme
                                        .onSurfaceVariant,
                                  ),
                        ),
                      Icon(Icons.lock_outline_rounded,
                          size: 11,
                          color:
                              Theme.of(context).colorScheme.onSurfaceVariant),
                      const SizedBox(width: 3),
                      Flexible(
                        child: Text(
                          context.l10n.chatE2eBadge,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style:
                              Theme.of(context).textTheme.labelSmall?.copyWith(
                                    color: Theme.of(context)
                                        .colorScheme
                                        .onSurfaceVariant,
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
        actions: [
          IconButton(
            tooltip: context.l10n.callAudio,
            onPressed: () => _startCall('audio'),
            icon: const Icon(Icons.call_outlined),
          ),
          IconButton(
            tooltip: context.l10n.callVideo,
            onPressed: () => _startCall('video'),
            icon: const Icon(Icons.videocam_outlined),
          ),
          PopupMenuButton<String>(
            tooltip: context.l10n.noteActions,
            onSelected: (value) {
              if (value == 'search') {
                setState(() {
                  _searchOpen = !_searchOpen;
                  if (!_searchOpen) {
                    _searchCtrl.clear();
                    _searchQuery = '';
                  }
                });
              } else if (value == 'refresh') {
                _loadMessages(reset: true);
                _loadThreadMeta();
              }
              if (value == 'rename') {
                _openRenameDialog();
              } else if (value == 'members') {
                _openMembersDialog();
              } else if (value == 'invite') {
                _copyInviteLink();
              } else if (value == 'unpin') {
                _unpinCurrent();
              } else if (value == 'leave') {
                _leaveChat();
              } else if (value == 'delete_group') {
                _deleteGroup();
              } else if (value == 'delete') {
                _deleteChat();
              }
            },
            itemBuilder: (context) => [
              PopupMenuItem(
                  value: 'search',
                  child: Text(context.l10n.chatSearchTooltip)),
              PopupMenuItem(
                  value: 'refresh', child: Text(context.l10n.chatRefresh)),
              PopupMenuItem(
                  value: 'members', child: Text(context.l10n.chatMembers)),
              if (widget.isGroup)
                PopupMenuItem(
                    value: 'rename', child: Text(context.l10n.chatRename)),
              if (widget.isGroup)
                PopupMenuItem(
                    value: 'invite', child: Text(context.l10n.chatInviteLink)),
              if (_canPin && _pin != null)
                PopupMenuItem(
                    value: 'unpin', child: Text(context.l10n.chatUnpinMessage)),
              if (widget.isGroup)
                PopupMenuItem(
                    value: 'leave', child: Text(context.l10n.chatLeaveGroup)),
              if (widget.isGroup && _myRole == 'owner')
                PopupMenuItem(
                    value: 'delete_group',
                    child: Text(context.l10n.chatDeleteGroup)),
              if (!widget.isGroup)
                PopupMenuItem(
                    value: 'delete',
                    child: Text(context.l10n.chatDeleteThread)),
            ],
          ),
        ],
      ),
      body: Column(
        children: [
          if (_searchOpen)
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 8, 12, 4),
              child: TextField(
                controller: _searchCtrl,
                autofocus: true,
                decoration: InputDecoration(
                  hintText: context.l10n.chatSearchHint,
                  prefixIcon: const Icon(Icons.search_rounded),
                  suffixIcon: _searchQuery.isEmpty
                      ? null
                      : IconButton(
                          onPressed: () {
                            _searchCtrl.clear();
                            setState(() => _searchQuery = '');
                          },
                          icon: const Icon(Icons.close_rounded),
                        ),
                  isDense: true,
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(UiTokens.radiusInput),
                  ),
                ),
                onChanged: _onSearchChanged,
              ),
            ),
          if (_searching) const LinearProgressIndicator(minHeight: 2),
          if (_pin != null) _buildPinBanner(),
          Expanded(child: _buildMessages()),
          if (_typingActors.isNotEmpty)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 4),
              child: Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  _typingLabel(context),
                  style: TextStyle(
                      color: Theme.of(context)
                          .colorScheme
                          .onSurface
                          .withAlpha(150),
                      fontSize: 12),
                ),
              ),
            ),
          _buildComposer(),
        ],
      ),
    );
  }

  String _typingLabel(BuildContext context) {
    final names = _typingActors.keys.toList();
    if (names.isEmpty) return '';
    if (names.length == 1) {
      return context.l10n.chatTyping(names.first);
    }
    if (names.length == 2) {
      return context.l10n.chatTypingMany('${names[0]} & ${names[1]}');
    }
    return context.l10n.chatTypingMany('${names[0]} +${names.length - 1}');
  }

  Widget _buildMessages() {
    if (_loading && _messages.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null && _messages.isEmpty) {
      return Padding(
        padding: const EdgeInsets.all(16),
        child: NetworkErrorCard(
          message: _error,
          onRetry: () => _loadMessages(reset: true),
        ),
      );
    }
    if (_messages.isEmpty) {
      return Center(child: Text(context.l10n.chatEmpty));
    }
    final selfActor = _selfActor();
    final query = _searchQuery.trim().toLowerCase();
    final visible = query.isEmpty
        ? _messages
        : (_searchResults.isNotEmpty
            ? _searchResults
            : _messages
                .where((m) => _messageText(m).toLowerCase().contains(query))
                .toList(growable: false));
    if (visible.isEmpty && query.isNotEmpty && !_searching) {
      return Center(child: Text(context.l10n.chatSearchNoResults));
    }
    return ListView.builder(
      reverse: true,
      itemCount: visible.length + (_next != null && query.isEmpty ? 1 : 0),
      itemBuilder: (context, index) {
        if (index >= visible.length) {
          return Padding(
            padding: const EdgeInsets.all(12),
            child: Center(
              child: OutlinedButton(
                onPressed: _loading ? null : () => _loadMessages(reset: false),
                child: Text(context.l10n.listLoadMore),
              ),
            ),
          );
        }
        final message = visible[index];
        final payload = message.payload;
        if (payload?.op == 'react') {
          return const SizedBox.shrink();
        }
        final isSelf = message.senderActor == selfActor;
        final text = _messageText(message);
        if (payload?.op == 'system') {
          return _systemNotice(context, payload!, message, isSelf);
        }
        final attachments = payload?.attachments ?? const [];
        final replyId = payload?.replyTo;
        final replyMsg = (replyId != null && replyId.isNotEmpty)
            ? _findMessageById(replyId)
            : null;
        final when = DateTime.fromMillisecondsSinceEpoch(message.createdAtMs);
        final summary = isSelf ? _statusMap[message.messageId] : null;
        final allSeen = summary != null &&
            _recipients.isNotEmpty &&
            summary.seen >= _recipients.length;
        final align =
            isSelf ? CrossAxisAlignment.end : CrossAxisAlignment.start;
        final radius = BorderRadius.only(
          topLeft: const Radius.circular(18),
          topRight: const Radius.circular(18),
          bottomLeft: Radius.circular(isSelf ? 18 : 4),
          bottomRight: Radius.circular(isSelf ? 4 : 18),
        );
        final senderProfile = _profiles[message.senderActor];
        final senderUsername =
            senderProfile?.preferredUsername.isNotEmpty == true
                ? senderProfile!.preferredUsername
                : _usernameFromActor(message.senderActor);
        final senderName = senderProfile?.displayName.isNotEmpty == true
            ? senderProfile!.displayName
            : senderUsername;
        final showSender = !isSelf && _recipients.length > 1;
        final timeColor = isSelf
            ? Colors.white.withAlpha(190)
            : Theme.of(context).colorScheme.onSurfaceVariant;
        return Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
          child: Row(
            mainAxisAlignment:
                isSelf ? MainAxisAlignment.end : MainAxisAlignment.start,
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              if (!isSelf) ...[
                StatusAvatar(
                  imageUrl: senderProfile?.iconUrl ?? '',
                  size: 30,
                  showStatus: true,
                  statusKey: senderUsername,
                ),
                const SizedBox(width: 6),
              ],
              Flexible(
                child: Column(
                  crossAxisAlignment: align,
                  children: [
                    if (showSender)
                      Padding(
                        padding: const EdgeInsets.only(left: 4, bottom: 3),
                        child: Text(
                          senderName,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 11.5,
                            fontWeight: FontWeight.w700,
                            color: Theme.of(context).colorScheme.primary,
                          ),
                        ),
                      ),
                    GestureDetector(
                      onLongPress: () async {
                        await showModalBottomSheet<void>(
                          context: context,
                          builder: (context) => SafeArea(
                            child: Wrap(
                              children: [
                                ListTile(
                                  leading:
                                      const Icon(Icons.emoji_emotions_outlined),
                                  title: Text(context.l10n.chatReact),
                                  onTap: () async {
                                    Navigator.of(context).pop();
                                    await _openReactionPicker(message);
                                  },
                                ),
                                ListTile(
                                  leading: const Icon(Icons.reply),
                                  title: Text(context.l10n.chatReply),
                                  onTap: () {
                                    Navigator.of(context).pop();
                                    setState(() => _replyTo = message);
                                  },
                                ),
                                ListTile(
                                  leading: const Icon(Icons.forward_rounded),
                                  title: Text(context.l10n.chatForward),
                                  onTap: () async {
                                    Navigator.of(context).pop();
                                    await _forwardMessage(message);
                                  },
                                ),
                                if (_canPin)
                                  ListTile(
                                    leading: Icon(
                                      _pin?['message_id'] == message.messageId
                                          ? Icons.push_pin
                                          : Icons.push_pin_outlined,
                                    ),
                                    title: Text(
                                      _pin?['message_id'] == message.messageId
                                          ? context.l10n.chatUnpinMessage
                                          : context.l10n.chatPinMessage,
                                    ),
                                    onTap: () async {
                                      Navigator.of(context).pop();
                                      await _togglePin(message);
                                    },
                                  ),
                                if (isSelf)
                                  ListTile(
                                    leading: const Icon(Icons.edit),
                                    title: Text(context.l10n.chatEdit),
                                    onTap: () async {
                                      Navigator.of(context).pop();
                                      await _editMessage(message);
                                    },
                                  ),
                                if (isSelf)
                                  ListTile(
                                    leading: const Icon(Icons.delete),
                                    title: Text(context.l10n.chatDelete),
                                    onTap: () async {
                                      Navigator.of(context).pop();
                                      await _deleteMessage(message);
                                    },
                                  ),
                              ],
                            ),
                          ),
                        );
                      },
                      child: Container(
                        decoration: BoxDecoration(
                          gradient: isSelf
                              ? const LinearGradient(
                                  begin: Alignment.topLeft,
                                  end: Alignment.bottomRight,
                                  colors: [
                                    Color(0xFF6D5DF5),
                                    Color(0xFF4C7BF4)
                                  ],
                                )
                              : null,
                          color: isSelf
                              ? null
                              : Theme.of(context)
                                  .colorScheme
                                  .surfaceContainerHighest,
                          borderRadius: radius,
                          boxShadow: [
                            BoxShadow(
                              color: Colors.black.withAlpha(isSelf ? 40 : 18),
                              blurRadius: 12,
                              offset: const Offset(0, 4),
                            ),
                          ],
                        ),
                        padding: const EdgeInsets.symmetric(
                            horizontal: 12, vertical: 8),
                        child: Column(
                          crossAxisAlignment: align,
                          children: [
                            if (replyId != null && replyId.isNotEmpty)
                              Container(
                                margin: const EdgeInsets.only(bottom: 6),
                                padding: const EdgeInsets.symmetric(
                                    horizontal: 8, vertical: 4),
                                decoration: BoxDecoration(
                                  color: isSelf
                                      ? Colors.white.withAlpha(30)
                                      : Theme.of(context)
                                          .colorScheme
                                          .surfaceContainerHighest,
                                  borderRadius: BorderRadius.circular(8),
                                ),
                                child: Text(
                                  replyMsg == null
                                      ? context.l10n.chatReplyUnknown
                                      : _replyPreview(replyMsg),
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: Theme.of(context).textTheme.labelSmall,
                                ),
                              ),
                            Text(
                              text,
                              style: TextStyle(
                                color: isSelf ? Colors.white : null,
                                height: 1.35,
                              ),
                            ),
                            if (attachments.isNotEmpty) ...[
                              const SizedBox(height: 8),
                              _buildAttachments(attachments,
                                  isSelf: isSelf),
                            ],
                            const SizedBox(height: 4),
                            Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text(
                                  formatTimeAgo(context, when),
                                  style:
                                      TextStyle(fontSize: 10, color: timeColor),
                                ),
                                if (message.editedAtMs != null && !message.deleted) ...[
                                  const SizedBox(width: 4),
                                  Text(
                                    context.l10n.chatEdited,
                                    style: TextStyle(
                                        fontSize: 10,
                                        fontStyle: FontStyle.italic,
                                        color: timeColor),
                                  ),
                                ],
                                if (isSelf) ...[
                                  const SizedBox(width: 5),
                                  Tooltip(
                                    message: summary == null
                                        ? context.l10n.chatStatusPending
                                        : allSeen
                                            ? context.l10n.chatStatusSeenAll
                                            : summary.tooltip(context),
                                    child: _DeliveryCheck(
                                      summary: summary,
                                      allSeen: allSeen,
                                      color: timeColor,
                                    ),
                                  ),
                                ],
                              ],
                            ),
                          ],
                        ),
                      ),
                    ),
                    _buildReactionRow(message, isSelf),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  Widget _buildReactionRow(ChatMessageItem message, bool isSelf) {
    final items = _reactions[message.messageId] ?? const [];
    if (items.isEmpty && message.deleted) return const SizedBox.shrink();
    final bg = Theme.of(context).colorScheme.surfaceContainerHighest;
    return Padding(
      padding: const EdgeInsets.only(top: 4),
      child: Wrap(
        spacing: 6,
        runSpacing: 6,
        alignment: isSelf ? WrapAlignment.end : WrapAlignment.start,
        children: [
          for (final r in items)
            InkWell(
              onTap: () => _sendReaction(message, r.reaction, remove: r.me),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                decoration: BoxDecoration(
                  color: r.me
                      ? Theme.of(context).colorScheme.primary.withAlpha(30)
                      : bg,
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(
                    color: r.me
                        ? Theme.of(context).colorScheme.primary
                        : Colors.transparent,
                    width: 1,
                  ),
                ),
                child: Text('${r.reaction} ${r.count}'),
              ),
            ),
          if (!message.deleted)
            InkWell(
              onTap: () => _openReactionPicker(message),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                decoration: BoxDecoration(
                  color: bg,
                  borderRadius: BorderRadius.circular(12),
                ),
                child: const Icon(Icons.add_reaction_outlined, size: 16),
              ),
            ),
        ],
      ),
    );
  }

  Widget _buildComposer() {
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (_replyTo != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Row(
                children: [
                  Expanded(
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 10, vertical: 6),
                      decoration: BoxDecoration(
                        color: Theme.of(context)
                            .colorScheme
                            .surfaceContainerHighest,
                        borderRadius: BorderRadius.circular(12),
                      ),
                      child: Text(
                        _replyPreview(_replyTo!),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ),
                  IconButton(
                    tooltip: context.l10n.chatReplyClear,
                    onPressed:
                        _sending ? null : () => setState(() => _replyTo = null),
                    icon: const Icon(Icons.close),
                  ),
                ],
              ),
            ),
          if (_media.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (var i = 0; i < _media.length; i++)
                    InputChip(
                      avatar: _media[i].bytes.isNotEmpty &&
                              _looksLikeImage(_media[i].name)
                          ? CircleAvatar(
                              backgroundImage: MemoryImage(_media[i].bytes),
                            )
                          : const Icon(Icons.insert_drive_file, size: 18),
                      label: Text(_media[i].name,
                          maxLines: 1, overflow: TextOverflow.ellipsis),
                      onDeleted: _sending
                          ? null
                          : () => setState(() => _media.removeAt(i)),
                    ),
                ],
              ),
            ),
          Container(
            padding: const EdgeInsets.fromLTRB(6, 4, 6, 4),
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.surfaceContainerHigh,
              borderRadius: BorderRadius.circular(28),
              border: Border.all(
                color: Theme.of(context).colorScheme.outlineVariant,
              ),
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                IconButton(
                  tooltip: context.l10n.composeAttachments,
                  visualDensity: VisualDensity.compact,
                  onPressed: _sending ? null : _openAttachMenu,
                  icon: const Icon(Icons.add_circle_outline_rounded, size: 22),
                ),
                Expanded(
                  child: TextField(
                    controller: _composerCtrl,
                    focusNode: _composerFocus,
                    minLines: 1,
                    maxLines: 5,
                    textCapitalization: TextCapitalization.sentences,
                    textInputAction: TextInputAction.send,
                    decoration: InputDecoration(
                      hintText: context.l10n.chatMessageHint,
                      filled: false,
                      isDense: true,
                      border: InputBorder.none,
                      enabledBorder: InputBorder.none,
                      focusedBorder: InputBorder.none,
                      contentPadding: const EdgeInsets.symmetric(vertical: 10),
                    ),
                    onSubmitted: (_) => _sendMessage(),
                  ),
                ),
                IconButton(
                  tooltip: context.l10n.composeEmojiButton,
                  visualDensity: VisualDensity.compact,
                  onPressed: _sending ? null : _insertEmoji,
                  icon: const Icon(Icons.emoji_emotions_outlined, size: 20),
                ),
                const SizedBox(width: 4),
                _ChatSendButton(
                  sending: _sending,
                  onPressed: _sending ? null : _sendMessage,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _openAttachMenu() async {
    final action = await showModalBottomSheet<String>(
      context: context,
      showDragHandle: true,
      builder: (context) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _AttachOption(
              icon: Icons.attach_file_rounded,
              label: context.l10n.composeAttachments,
              value: 'file',
            ),
            _AttachOption(
              icon: Icons.mic_none_rounded,
              label: context.l10n.chatAudioNote,
              value: 'audio',
            ),
            _AttachOption(
              icon: Icons.videocam_outlined,
              label: context.l10n.chatVideoNote,
              value: 'video',
            ),
            _AttachOption(
              icon: Icons.gif_box_outlined,
              label: context.l10n.chatGif,
              value: 'gif',
            ),
          ],
        ),
      ),
    );
    if (!mounted || action == null) return;
    switch (action) {
      case 'audio':
        await _recordNote(false);
      case 'video':
        await _recordNote(true);
      case 'gif':
        await _openGifPicker();
      default:
        await _pickFiles();
    }
  }

  Widget _systemNotice(
      BuildContext context, ChatPayload payload, ChatMessageItem message,
      bool isSelf) {
    final label = switch (payload.action) {
      'create_group' => context.l10n.chatGroupCreated,
      'rename' => context.l10n.chatGroupRenamed,
      'add_member' => context.l10n.chatMemberAdded,
      'remove_member' => context.l10n.chatMemberRemoved,
      'leave' => context.l10n.chatMemberLeft,
      'delete_thread' => context.l10n.chatThreadDeleted,
      _ => payload.text?.trim() ?? '',
    };
    if (label.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
      child: Center(
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
          decoration: BoxDecoration(
            color: Theme.of(context).colorScheme.surfaceContainerHigh,
            borderRadius: BorderRadius.circular(14),
          ),
          child: Text(
            label,
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.labelSmall?.copyWith(
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
          ),
        ),
      ),
    );
  }

  void _focusComposer() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _composerFocus.requestFocus();
      final length = _composerCtrl.text.length;
      _composerCtrl.selection = TextSelection.collapsed(offset: length);
    });
  }

  KeyEventResult _handleComposerKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    if (key != LogicalKeyboardKey.enter &&
        key != LogicalKeyboardKey.numpadEnter) {
      return KeyEventResult.ignored;
    }
    if (HardwareKeyboard.instance.isAltPressed) {
      _insertNewline();
      return KeyEventResult.handled;
    }
    unawaited(_sendMessage());
    return KeyEventResult.handled;
  }

  void _insertNewline() {
    final text = _composerCtrl.text;
    final selection = _composerCtrl.selection;
    final start = selection.isValid ? selection.start : text.length;
    final end = selection.isValid ? selection.end : text.length;
    final next = text.replaceRange(start, end, '\n');
    _composerCtrl.value = TextEditingValue(
      text: next,
      selection: TextSelection.collapsed(offset: start + 1),
    );
  }

  String _messageText(ChatMessageItem message) {
    if (message.deleted) return context.l10n.chatMessageDeleted;
    final payload = message.payload;
    if (payload == null) return message.bodyJson;
    if (payload.op == 'message' || payload.op == 'edit') {
      final text = payload.text?.trim() ?? '';
      if (text.isNotEmpty) return text;
      if ((payload.attachments?.isNotEmpty ?? false)) return '';
      return context.l10n.chatMessageEmpty;
    }
    return payload.text ?? context.l10n.chatMessageEmpty;
  }

  String _replyPreview(ChatMessageItem message) {
    final payload = message.payload;
    if (payload == null) return message.bodyJson;
    final text = payload.text?.trim();
    if (text != null && text.isNotEmpty) return text;
    if (payload.attachments?.isNotEmpty ?? false) {
      return context.l10n.chatReplyAttachment;
    }
    return context.l10n.chatMessageEmpty;
  }

  ChatMessageItem? _findMessageById(String id) {
    for (final m in _messages) {
      if (m.messageId == id) return m;
    }
    return null;
  }

  Widget _buildAttachments(List<ChatAttachment> attachments,
      {bool isSelf = false}) {
    final audio = attachments.where((a) => _isAudio(a.mediaType, a.url)).toList();
    final media = attachments
        .where((a) => _isMedia(a.mediaType, a.url) && !_isAudio(a.mediaType, a.url))
        .toList();
    final docs =
        attachments.where((a) => !_isMedia(a.mediaType, a.url)).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final a in audio)
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: _ChatAudioTile(
              url: _resolveMediaUrl(a.url),
              isSelf: isSelf,
            ),
          ),
        if (media.isNotEmpty)
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final a in media)
                SizedBox(
                  width: 180,
                  height: 120,
                  child: InlineMediaTile(
                    url: _resolveMediaUrl(a.url),
                    mediaType: a.mediaType,
                    cacheWidth: 320,
                    autoplay: false,
                    muted: true,
                    onOpen: () => _openMediaViewer(attachments, a),
                  ),
                ),
            ],
          ),
        if (docs.isNotEmpty) ...[
          if (media.isNotEmpty) const SizedBox(height: 8),
          for (final a in docs)
            ListTile(
              dense: true,
              contentPadding: EdgeInsets.zero,
              leading: const Icon(Icons.insert_drive_file),
              title: Text(a.name ?? a.url,
                  maxLines: 1, overflow: TextOverflow.ellipsis),
              onTap: () => openUrlExternal(a.url),
            ),
        ],
      ],
    );
  }

  bool _looksLikeImage(String name) {
    final n = name.toLowerCase();
    return n.endsWith('.png') ||
        n.endsWith('.jpg') ||
        n.endsWith('.jpeg') ||
        n.endsWith('.gif') ||
        n.endsWith('.webp');
  }

  bool _isAudio(String mediaType, String url) {
    final mt = mediaType.toLowerCase();
    if (mt.startsWith('audio/')) return true;
    final u = url.toLowerCase();
    return u.endsWith('.m4a') ||
        u.endsWith('.mp3') ||
        u.endsWith('.ogg') ||
        u.endsWith('.opus') ||
        u.endsWith('.wav') ||
        u.endsWith('.aac');
  }

  bool _isMedia(String mediaType, String url) {
    final mt = mediaType.toLowerCase();
    if (mt.startsWith('image/') ||
        mt.startsWith('video/') ||
        mt.startsWith('audio/')) {
      return true;
    }
    final u = url.toLowerCase();
    return u.endsWith('.png') ||
        u.endsWith('.jpg') ||
        u.endsWith('.jpeg') ||
        u.endsWith('.gif') ||
        u.endsWith('.webp') ||
        u.endsWith('.mp4') ||
        u.endsWith('.webm') ||
        u.endsWith('.mov') ||
        u.endsWith('.mp3') ||
        u.endsWith('.ogg') ||
        u.endsWith('.wav');
  }

  void _openMediaViewer(
      List<ChatAttachment> attachments, ChatAttachment focus) {
    final items = attachments
        .where((a) => _isMedia(a.mediaType, a.url))
        .map((a) => NoteAttachment(url: a.url, mediaType: a.mediaType))
        .toList();
    if (items.isEmpty) {
      openUrlExternal(focus.url);
      return;
    }
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => MediaViewerScreen(
          appState: widget.appState,
          url: focus.url,
          mediaType: focus.mediaType,
          attachments: items,
        ),
      ),
    );
  }
}

class _ChatSendButton extends StatelessWidget {
  const _ChatSendButton({required this.sending, required this.onPressed});

  final bool sending;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null;
    return Padding(
      padding: const EdgeInsets.only(bottom: 2),
      child: Opacity(
        opacity: enabled ? 1 : 0.5,
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            borderRadius: BorderRadius.circular(999),
            onTap: onPressed,
            child: Container(
              width: 40,
              height: 40,
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
                boxShadow: [
                  BoxShadow(
                    color: Color(0x667C6CF6),
                    blurRadius: 12,
                    offset: Offset(0, 4),
                  ),
                ],
              ),
              child: sending
                  ? const Padding(
                      padding: EdgeInsets.all(10),
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: Colors.white,
                      ),
                    )
                  : const Icon(Icons.send_rounded,
                      color: Colors.white, size: 20),
            ),
          ),
        ),
      ),
    );
  }
}

class _DeliveryCheck extends StatelessWidget {
  const _DeliveryCheck({
    required this.summary,
    required this.allSeen,
    required this.color,
  });

  final _StatusSummary? summary;
  final bool allSeen;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final icon = summary?.icon ?? Icons.schedule;
    final seenColor = allSeen ? const Color(0xFF67E8F9) : color;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Icon(icon, size: 15, color: seenColor),
        if (allSeen)
          Positioned(
            right: -4,
            top: -4,
            child: Container(
              width: 8,
              height: 8,
              decoration: const BoxDecoration(
                color: Color(0xFF67E8F9),
                shape: BoxShape.circle,
              ),
              child: const Icon(
                Icons.add,
                size: 6,
                color: Color(0xFF06121A),
              ),
            ),
          ),
      ],
    );
  }
}

class _PickedMedia {
  _PickedMedia({required this.name, required this.bytes});

  final String name;
  final Uint8List bytes;
  String? coreMediaId;
  String? url;
  String? mediaType;
}

class _StatusSummary {
  int sent = 0;
  int delivered = 0;
  int seen = 0;
  int queued = 0;

  void add(String status) {
    final s = status.toLowerCase();
    if (s == 'seen') {
      seen += 1;
    } else if (s == 'delivered') {
      delivered += 1;
    } else if (s == 'sent') {
      sent += 1;
    } else if (s == 'queued') {
      queued += 1;
    }
  }

  IconData get icon {
    if (seen > 0) return Icons.done_all;
    if (delivered > 0) return Icons.done_all;
    if (sent > 0) return Icons.check;
    if (queued > 0) return Icons.schedule;
    return Icons.schedule;
  }

  String get bestStatus {
    if (seen > 0) return 'seen';
    if (delivered > 0) return 'delivered';
    if (sent > 0) return 'sent';
    if (queued > 0) return 'queued';
    return '';
  }

  String tooltip(BuildContext context) {
    if (seen > 0) return context.l10n.chatStatusSeen;
    if (delivered > 0) return context.l10n.chatStatusDelivered;
    if (sent > 0) return context.l10n.chatStatusSent;
    if (queued > 0) return context.l10n.chatStatusQueued;
    return context.l10n.chatStatusPending;
  }
}

class _ReactionItem {
  _ReactionItem({
    required this.reaction,
    required this.count,
    required this.me,
  });

  final String reaction;
  final int count;
  final bool me;
}

class _GifPicker extends StatefulWidget {
  const _GifPicker({required this.prefs, required this.onPick});

  final UiPrefs prefs;
  final Future<void> Function(GifResult) onPick;

  @override
  State<_GifPicker> createState() => _GifPickerState();
}

class _GifPickerState extends State<_GifPicker> {
  final TextEditingController _search = TextEditingController();
  bool _loading = false;
  bool _missingKey = false;
  List<GifResult> _items = const [];
  Timer? _debounce;
  final List<String> _suggestions = const [
    'funny',
    'love',
    'wow',
    'party',
    'sad',
    'cat',
    'dog'
  ];

  @override
  void initState() {
    super.initState();
    _load();
    _search.addListener(_onQuery);
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _search.dispose();
    super.dispose();
  }

  void _onQuery() {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 300), _load);
  }

  Future<void> _load() async {
    final query = _search.text.trim();
    if (widget.prefs.gifApiKey.trim().isEmpty) {
      if (mounted) {
        setState(() {
          _missingKey = true;
          _items = const [];
          _loading = false;
        });
      }
      return;
    }
    _missingKey = false;
    setState(() => _loading = true);
    try {
      final items = await GifService.search(
        query,
        apiKey: widget.prefs.gifApiKey,
      );
      if (mounted) setState(() => _items = items);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final height = MediaQuery.of(context).size.height * 0.6;
    return SafeArea(
      child: SizedBox(
        height: height,
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
              child: TextField(
                controller: _search,
                decoration: InputDecoration(
                  hintText: context.l10n.chatGifSearchHint,
                  prefixIcon: const Icon(Icons.search),
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Wrap(
                spacing: 8,
                runSpacing: 6,
                children: [
                  for (final s in _suggestions)
                    ActionChip(
                      label: Text(s),
                      onPressed: () {
                        _search.text = s;
                        _search.selection =
                            TextSelection.collapsed(offset: s.length);
                        _load();
                      },
                    ),
                ],
              ),
            ),
            if (_loading) const LinearProgressIndicator(minHeight: 2),
            Expanded(
              child: _missingKey
                  ? Center(child: Text(context.l10n.chatGifMissingKey))
                  : _items.isEmpty
                      ? Center(child: Text(context.l10n.chatGifEmpty))
                      : GridView.builder(
                          padding: const EdgeInsets.all(12),
                          gridDelegate:
                              const SliverGridDelegateWithFixedCrossAxisCount(
                            crossAxisCount: 3,
                            crossAxisSpacing: 8,
                            mainAxisSpacing: 8,
                          ),
                          itemCount: _items.length,
                          itemBuilder: (context, index) {
                            final gif = _items[index];
                            return InkWell(
                              onTap: () => widget.onPick(gif),
                              child: ClipRRect(
                                borderRadius: BorderRadius.circular(10),
                                child: Image.network(
                                  gif.previewUrl,
                                  fit: BoxFit.cover,
                                  filterQuality: FilterQuality.low,
                                  errorBuilder: (_, __, ___) => Container(
                                    color: Theme.of(context)
                                        .colorScheme
                                        .surfaceContainerHighest,
                                    child:
                                        const Icon(Icons.broken_image_outlined),
                                  ),
                                ),
                              ),
                            );
                          },
                        ),
            ),
          ],
        ),
      ),
    );
  }
}

class _AttachOption extends StatelessWidget {
  const _AttachOption({
    required this.icon,
    required this.label,
    required this.value,
  });

  final IconData icon;
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      leading: Icon(icon),
      title: Text(label),
      onTap: () => Navigator.of(context).pop(value),
    );
  }
}

/// Voice note bubble: play/pause, progress and duration for audio attachments.
class _ChatAudioTile extends StatefulWidget {
  const _ChatAudioTile({required this.url, this.isSelf = false});

  final String url;
  final bool isSelf;

  @override
  State<_ChatAudioTile> createState() => _ChatAudioTileState();
}

class _ChatAudioTileState extends State<_ChatAudioTile> {
  final AudioPlayer _player = AudioPlayer();
  Duration _duration = Duration.zero;
  Duration _position = Duration.zero;
  bool _playing = false;
  bool _loading = false;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    // Load the source up front so the duration is visible before playback.
    unawaited(_prepare());
    _player.onDurationChanged.listen((d) {
      if (mounted) setState(() => _duration = d);
    });
    _player.onPositionChanged.listen((p) {
      if (mounted) setState(() => _position = p);
    });
    _player.onPlayerStateChanged.listen((state) {
      if (!mounted) return;
      setState(() => _playing = state == PlayerState.playing);
    });
    _player.onPlayerComplete.listen((_) {
      if (!mounted) return;
      setState(() {
        _playing = false;
        _position = Duration.zero;
      });
    });
  }

  @override
  void dispose() {
    _player.dispose();
    super.dispose();
  }

  Future<void> _prepare() async {
    try {
      await _player.setSourceUrl(widget.url);
      final duration = await _player.getDuration();
      if (mounted && duration != null && duration > Duration.zero) {
        setState(() => _duration = duration);
      }
    } catch (_) {
      if (mounted) setState(() => _failed = true);
    }
  }

  Future<void> _toggle() async {
    if (_failed) return;
    try {
      if (_playing) {
        await _player.pause();
        return;
      }
      if (_duration > Duration.zero && _position >= _duration) {
        await _player.seek(Duration.zero);
      }
      setState(() => _loading = true);
      await _player.resume();
      if (mounted) setState(() => _loading = false);
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _failed = true;
      });
    }
  }

  String _label(Duration d) {
    final total = d.inSeconds;
    final m = (total ~/ 60).toString().padLeft(2, '0');
    final s = (total % 60).toString().padLeft(2, '0');
    return '$m:$s';
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final fg = widget.isSelf ? Colors.white : scheme.onSurface;
    final track = widget.isSelf
        ? Colors.white.withAlpha(70)
        : scheme.outlineVariant;
    final total = _duration.inMilliseconds;
    final progress = total <= 0
        ? 0.0
        : (_position.inMilliseconds / total).clamp(0.0, 1.0);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: widget.isSelf
            ? Colors.white.withAlpha(28)
            : scheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(14),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 40,
            height: 40,
            child: IconButton(
              padding: EdgeInsets.zero,
              tooltip: _playing ? 'Pause' : 'Play',
              onPressed: _loading ? null : _toggle,
              icon: _loading
                  ? SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: fg,
                      ),
                    )
                  : Icon(
                      _failed
                          ? Icons.error_outline_rounded
                          : _playing
                              ? Icons.pause_rounded
                              : Icons.play_arrow_rounded,
                      color: fg,
                      size: 26,
                    ),
            ),
          ),
          const SizedBox(width: 4),
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                width: 150,
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(4),
                  child: LinearProgressIndicator(
                    value: progress,
                    minHeight: 4,
                    backgroundColor: track,
                    color: fg,
                  ),
                ),
              ),
              const SizedBox(height: 4),
              Text(
                _duration == Duration.zero
                    ? 'Audio'
                    : '${_label(_position)} / ${_label(_duration)}',
                style: TextStyle(fontSize: 10, color: fg.withAlpha(190)),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
