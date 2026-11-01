/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

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
import 'chat_thread_screen.dart';

class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  bool _loading = false;
  String? _error;
  String? _next;
  List<ChatThreadItem> _threads = const [];
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
        .savePrefs(widget.appState.prefs.copyWith(lastChatSeenMs: now));
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
      _loadThreads(reset: true);
    });
  }

  void _updateUnreadCounts(List<ChatThreadItem> items) {
    final seen = widget.appState.prefs.chatThreadSeenMs;
    var unread = 0;
    for (final t in items) {
      final lastMs = t.lastMessageMs ?? t.updatedAtMs;
      final seenMs = seen[t.threadId] ?? 0;
      if (lastMs > seenMs) unread += 1;
    }
    widget.appState.setUnreadChats(unread);
  }

  Future<void> _openNewChatDialog() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final outerContext = context;
    final threadId = await showDialog<String>(
      context: context,
      builder: (_) => _NewChatDialog(config: cfg),
    );

    if (threadId != null && threadId.isNotEmpty && outerContext.mounted) {
      await Navigator.of(outerContext).push(
        MaterialPageRoute(
          builder: (_) => ChatThreadScreen(
            appState: widget.appState,
            threadId: threadId,
            title: outerContext.l10n.chatThreadTitle,
          ),
        ),
      );
      if (mounted) {
        await _loadThreads(reset: true);
      }
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
        final name = thread.title?.trim().isNotEmpty == true
            ? thread.title!.trim()
            : (thread.kind == 'dm'
                ? context.l10n.chatDirectMessage
                : context.l10n.chatGroup);
        final preview = thread.lastMessagePreview?.trim().isNotEmpty == true
            ? thread.lastMessagePreview!.trim()
            : context.l10n.chatNoMessages;
        final ts = thread.lastMessageMs ?? thread.updatedAtMs;
        final when =
            ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts) : DateTime.now();
        final seenMs =
            widget.appState.prefs.chatThreadSeenMs[thread.threadId] ?? 0;
        final isUnread = ts > seenMs;
        final isPinned = pinnedIds.contains(thread.threadId);
        return Padding(
          padding: const EdgeInsets.only(bottom: 10),
          child: Card(
            child: InkWell(
              borderRadius: BorderRadius.circular(UiTokens.radiusCard),
              onLongPress: () => _showThreadMenu(thread, isPinned),
              onTap: () async {
                await Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (_) => ChatThreadScreen(
                      appState: widget.appState,
                      threadId: thread.threadId,
                      title: name,
                    ),
                  ),
                );
                if (mounted) {
                  await _loadThreads(reset: true);
                }
              },
              child: Padding(
                padding: const EdgeInsets.all(14),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
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
                                  style: const TextStyle(
                                      fontWeight: FontWeight.w800),
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
                                        ? Theme.of(context)
                                            .colorScheme
                                            .onSurface
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
            ),
          ),
        );
      },
    );
  }

  Future<void> _showThreadMenu(ChatThreadItem thread, bool pinned) async {
    final prefs = widget.appState.prefs;
    final nextPinned = List<String>.from(prefs.pinnedChatThreads);
    final action = await showModalBottomSheet<String>(
      context: context,
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
            ],
          ),
        );
      },
    );
    if (action != 'pin') return;
    if (pinned) {
      nextPinned.removeWhere((id) => id == thread.threadId);
    } else {
      if (!nextPinned.contains(thread.threadId)) {
        nextPinned.add(thread.threadId);
      }
    }
    await widget.appState
        .savePrefs(prefs.copyWith(pinnedChatThreads: nextPinned));
    if (mounted) setState(() {});
  }
}

class _NewChatDialog extends StatefulWidget {
  const _NewChatDialog({required this.config});

  final CoreConfig config;

  @override
  State<_NewChatDialog> createState() => _NewChatDialogState();
}

class _NewChatDialogState extends State<_NewChatDialog> {
  final TextEditingController _recipientsCtrl = TextEditingController();
  final TextEditingController _messageCtrl = TextEditingController();
  String? _error;
  bool _sending = false;
  bool _searching = false;
  List<ActorProfile> _suggestions = const [];
  Timer? _searchDebounce;

  @override
  void dispose() {
    _searchDebounce?.cancel();
    _recipientsCtrl.dispose();
    _messageCtrl.dispose();
    super.dispose();
  }

  void _runSearch(String raw) {
    _searchDebounce?.cancel();
    _searchDebounce = Timer(const Duration(milliseconds: 250), () async {
      final parts = raw.split(RegExp(r'[,\n]'));
      final last = parts.isNotEmpty ? parts.last.trim() : '';
      if (last.length < 2) {
        if (mounted) setState(() => _suggestions = const []);
        return;
      }
      if (mounted) setState(() => _searching = true);
      try {
        final api = CoreApi(config: widget.config);
        final resp = await api.searchUsers(
          query: last,
          source: 'all',
          consistency: 'best',
          limit: 6,
        );
        final rawItems = resp['items'];
        final list = <ActorProfile>[];
        if (rawItems is List) {
          for (final it in rawItems) {
            if (it is! Map) continue;
            final profile = ActorProfile.tryParse(it.cast<String, dynamic>());
            if (profile != null) list.add(profile);
          }
        }
        if (mounted) {
          setState(() => _suggestions = list);
        }
      } catch (_) {
        if (mounted) setState(() => _suggestions = const []);
      } finally {
        if (mounted) setState(() => _searching = false);
      }
    });
  }

  Future<void> _submit() async {
    final l10n = context.l10n;
    final recipientsRaw = _recipientsCtrl.text.trim();
    final message = _messageCtrl.text.trim();
    if (recipientsRaw.isEmpty || message.isEmpty) {
      setState(() => _error = l10n.chatNewMissingFields);
      return;
    }
    setState(() {
      _error = null;
      _sending = true;
    });
    try {
      final api = CoreApi(config: widget.config);
      final parts = recipientsRaw
          .split(RegExp(r'[,\n]'))
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList();
      if (parts.isEmpty) {
        throw StateError(l10n.chatNewMissingFields);
      }
      final resolved = <String>[];
      for (final p in parts) {
        resolved.add(await api.resolveActorInput(p));
      }
      final resp =
          await api.sendChatMessage(recipients: resolved, text: message);
      final createdId = resp['thread_id']?.toString();
      if (createdId == null || createdId.isEmpty) {
        throw StateError(l10n.chatNewFailed);
      }
      if (!mounted) return;
      Navigator.of(context).pop(createdId);
    } catch (e) {
      if (mounted) {
        setState(() => _error = e.toString());
      }
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(context.l10n.chatNewTitle),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _recipientsCtrl,
              decoration: InputDecoration(
                labelText: context.l10n.chatRecipients,
                hintText: context.l10n.chatRecipientsHint,
              ),
              onChanged: _runSearch,
              textInputAction: TextInputAction.next,
            ),
            if (_searching)
              const Padding(
                padding: EdgeInsets.only(top: 8),
                child: LinearProgressIndicator(minHeight: 2),
              ),
            if (_suggestions.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Container(
                  constraints: const BoxConstraints(maxHeight: 160),
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(12),
                    color:
                        Theme.of(context).colorScheme.surfaceContainerHighest,
                  ),
                  child: ListView.separated(
                    shrinkWrap: true,
                    itemCount: _suggestions.length,
                    separatorBuilder: (_, __) => const Divider(height: 1),
                    itemBuilder: (context, index) {
                      final profile = _suggestions[index];
                      return ListTile(
                        dense: true,
                        title: Text(profile.displayName,
                            maxLines: 1, overflow: TextOverflow.ellipsis),
                        subtitle: Text(profile.id,
                            maxLines: 1, overflow: TextOverflow.ellipsis),
                        onTap: () {
                          final raw = _recipientsCtrl.text;
                          final parts = raw.split(RegExp(r'[,\n]'));
                          final last = parts.isNotEmpty ? parts.last : '';
                          final prefix =
                              raw.substring(0, raw.length - last.length);
                          final next = '$prefix${profile.id}, ';
                          _recipientsCtrl.text = next;
                          _recipientsCtrl.selection =
                              TextSelection.fromPosition(
                            TextPosition(offset: next.length),
                          );
                          setState(() => _suggestions = const []);
                        },
                      );
                    },
                  ),
                ),
              ),
            const SizedBox(height: 12),
            TextField(
              controller: _messageCtrl,
              decoration: InputDecoration(
                labelText: context.l10n.chatMessage,
                hintText: context.l10n.chatMessageHint,
              ),
              minLines: 2,
              maxLines: 6,
            ),
            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(_error!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error)),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _sending ? null : () => Navigator.of(context).pop(),
          child: Text(context.l10n.cancel),
        ),
        FilledButton(
          onPressed: _sending ? null : _submit,
          child: _sending
              ? const SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2))
              : Text(context.l10n.chatCreate),
        ),
      ],
    );
  }
}
