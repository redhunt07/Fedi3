/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:file_selector/file_selector.dart';
import 'package:mime/mime.dart';

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
import '../utils/time_ago.dart';
import '../utils/open_url.dart';
import '../utils/media_url.dart';
import '../widgets/network_error_card.dart';
import '../widgets/inline_media_tile.dart';
import '../screens/media_viewer_screen.dart';
import '../widgets/emoji_picker.dart';
import 'package:http/http.dart' as http;

class ChatThreadScreen extends StatefulWidget {
  const ChatThreadScreen({
    super.key,
    required this.appState,
    required this.threadId,
    required this.title,
  });

  final AppState appState;
  final String threadId;
  final String title;

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
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  Timer? _loadRetry;
  Timer? _typingDebounce;
  final List<_PickedMedia> _media = [];
  bool _sending = false;
  Map<String, _StatusSummary> _statusMap = const {};
  Map<String, List<_ReactionItem>> _reactions = const {};
  ChatMessageItem? _replyTo;
  final Map<String, int> _typingActors = {};
  int _lastTypingSentMs = 0;
  CoreConfig? _streamConfig;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;

  @override
  void initState() {
    super.initState();
    _title = widget.title;
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadMessages(reset: true));
    _markSeen();
    _markThreadSeen();
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
    }
  }

  @override
  void dispose() {
    _streamSub?.cancel();
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
    _loadRetry?.cancel();
    _typingDebounce?.cancel();
    _composerCtrl.dispose();
    widget.appState.removeListener(_appStateListener);
    super.dispose();
  }

  Future<void> _markSeen() async {
    final now = DateTime.now().millisecondsSinceEpoch;
    await widget.appState.savePrefs(widget.appState.prefs.copyWith(lastChatSeenMs: now));
    widget.appState.clearUnreadChats();
  }

  Future<void> _markThreadSeen() async {
    final prefs = widget.appState.prefs;
    final now = DateTime.now().millisecondsSinceEpoch;
    final next = Map<String, int>.from(prefs.chatThreadSeenMs);
    next[widget.threadId] = now;
    await widget.appState.savePrefs(prefs.copyWith(chatThreadSeenMs: next));
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
      if (ty == 'react') {
        final ids = _messages.map((m) => m.messageId).where((id) => id.isNotEmpty).toList();
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

  String _selfActor() {
    final cfg = widget.appState.config;
    if (cfg == null) return '';
    return '${cfg.publicBaseUrl.replaceAll(RegExp(r"/+$"), "")}/users/${cfg.username}';
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
      }
      if (items.isNotEmpty) {
        final latest = items.first;
        await api.markChatSeen(threadId: latest.threadId, messageId: latest.messageId);
        await _markSeen();
        await _markThreadSeen();
      }
      final ids = items.map((m) => m.messageId).where((id) => id.isNotEmpty).toList();
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
      final map = <String, _StatusSummary>{};
      for (final it in raw) {
        if (it is! Map) continue;
        final msgId = it['message_id']?.toString() ?? '';
        final status = it['status']?.toString() ?? '';
        if (msgId.isEmpty || status.isEmpty) continue;
        final current = map[msgId] ?? _StatusSummary();
        current.add(status);
        map[msgId] = current;
      }
      if (mounted) setState(() => _statusMap = map);
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
            reactions.add(_ReactionItem(reaction: reaction, count: count, me: me));
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

  Future<void> _sendReaction(ChatMessageItem message, String reaction, {required bool remove}) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      await CoreApi(config: cfg).sendChatReaction(
        messageId: message.messageId,
        reaction: reaction,
        remove: remove,
      );
      final ids = _messages.map((m) => m.messageId).where((id) => id.isNotEmpty).toList();
      if (ids.isNotEmpty) {
        await _loadReactions(ids);
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _sendMessage() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final text = _composerCtrl.text.trim();
    if (text.isEmpty && _media.isEmpty) return;
    _composerCtrl.clear();
    setState(() => _sending = true);
    try {
      final api = CoreApi(config: cfg);
      final attachments = <Map<String, dynamic>>[];
      for (final m in _media) {
        if (m.coreMediaId == null || m.url == null || m.mediaType == null) {
          final resp = await api.uploadMedia(bytes: m.bytes, filename: m.name);
          final id = (resp['id'] as String?)?.trim();
          final url = (resp['url'] as String?)?.trim();
          final mediaType = (resp['media_type'] as String?)?.trim() ?? lookupMimeType(m.name) ?? '';
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
        replyTo: _replyTo?.messageId,
      );
      _replyTo = null;
      _media.clear();
      await _loadMessages(reset: true);
    } catch (e) {
      if (mounted) {
        setState(() => _error = e.toString());
      }
    } finally {
      if (mounted) setState(() => _sending = false);
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
    _composerCtrl.selection = TextSelection.collapsed(offset: insertAt + emoji.length);
  }

  Future<void> _openRenameDialog() async {
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
          TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(context.l10n.cancel)),
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
      await CoreApi(config: cfg).updateChatThreadTitle(threadId: widget.threadId, title: result);
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
    List<ActorProfile> suggestions = const [];
    bool loading = true;
    bool working = false;
    String? error;
    Timer? debounce;

    Future<void> loadMembers() async {
      try {
        final resp = await api.fetchChatMembers(threadId: widget.threadId);
        final raw = resp['items'];
        final list = <String>[];
        if (raw is List) {
          for (final it in raw) {
            if (it is! Map) continue;
            final id = it['actor_id']?.toString() ?? '';
            if (id.isNotEmpty) list.add(id);
          }
        }
        members = list;
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
                      final profile = ActorProfile.tryParse(it.cast<String, dynamic>());
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
                await api.updateChatThreadMembers(threadId: widget.threadId, add: [actor]);
                if (!members.contains(actor)) {
                  members = [...members, actor];
                }
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
                await api.updateChatThreadMembers(threadId: widget.threadId, remove: [actor]);
                members = members.where((m) => m != actor).toList();
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
                            color: Theme.of(context).colorScheme.surfaceContainerHighest,
                          ),
                          child: ListView.separated(
                            shrinkWrap: true,
                            itemCount: suggestions.length,
                            separatorBuilder: (_, __) => const Divider(height: 1),
                            itemBuilder: (context, index) {
                              final profile = suggestions[index];
                              return ListTile(
                                dense: true,
                                title: Text(profile.displayName, maxLines: 1, overflow: TextOverflow.ellipsis),
                                subtitle: Text(profile.id, maxLines: 1, overflow: TextOverflow.ellipsis),
                                onTap: () => addMember(profile.id),
                              );
                            },
                          ),
                        ),
                      ),
                    const SizedBox(height: 12),
                    if (error != null)
                      Text(error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
                    const SizedBox(height: 8),
                    SizedBox(
                      height: 200,
                      child: ListView.builder(
                        itemCount: members.length,
                        itemBuilder: (context, index) {
                          final actor = members[index];
                          return ListTile(
                            dense: true,
                            title: Text(actor, maxLines: 1, overflow: TextOverflow.ellipsis),
                            trailing: IconButton(
                              tooltip: context.l10n.chatRemoveMember,
                              onPressed: working ? null : () => removeMember(actor),
                              icon: const Icon(Icons.person_remove),
                            ),
                          );
                        },
                      ),
                    ),
                  ],
                ),
              ),
              actions: [
                TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(context.l10n.cancel)),
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
              setState(() => _media.add(_PickedMedia(name: name, bytes: bytes)));
            } catch (e) {
              if (mounted) setState(() => _error = e.toString());
            }
          },
        );
      },
    );
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
            TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(context.l10n.cancel)),
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
    await CoreApi(config: cfg).editChatMessage(messageId: message.messageId, text: result.trim());
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
          TextButton(onPressed: () => Navigator.of(context).pop(false), child: Text(context.l10n.cancel)),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.chatDelete),
          ),
        ],
      ),
    );
    if (ok != true) return;
    await CoreApi(config: cfg).deleteChatMessage(messageId: message.messageId);
    await _loadMessages(reset: true);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_title),
        actions: [
          IconButton(
            tooltip: context.l10n.chatRefresh,
            onPressed: _loading ? null : () => _loadMessages(reset: true),
            icon: const Icon(Icons.refresh),
          ),
          PopupMenuButton<String>(
            onSelected: (value) {
              if (value == 'rename') {
                _openRenameDialog();
              } else if (value == 'members') {
                _openMembersDialog();
              }
            },
            itemBuilder: (context) => [
              PopupMenuItem(value: 'members', child: Text(context.l10n.chatMembers)),
              PopupMenuItem(value: 'rename', child: Text(context.l10n.chatRename)),
            ],
          ),
        ],
      ),
      body: Column(
        children: [
          Expanded(child: _buildMessages()),
          if (_typingActors.isNotEmpty)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 4),
              child: Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  _typingLabel(context),
                  style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(150), fontSize: 12),
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
    return ListView.builder(
      reverse: true,
      itemCount: _messages.length + (_next != null ? 1 : 0),
      itemBuilder: (context, index) {
        if (index >= _messages.length) {
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
        final message = _messages[index];
        final payload = message.payload;
        if (payload?.op == 'react') {
          return const SizedBox.shrink();
        }
        final isSelf = message.senderActor == selfActor;
        final text = _messageText(message);
        final attachments = payload?.attachments ?? const [];
        final replyId = payload?.replyTo;
        final replyMsg = (replyId != null && replyId.isNotEmpty) ? _findMessageById(replyId) : null;
        final when = DateTime.fromMillisecondsSinceEpoch(message.createdAtMs);
        final status = isSelf ? _statusMap[message.messageId] : null;
        final bubbleColor = isSelf
            ? Theme.of(context).colorScheme.primaryContainer
            : Theme.of(context).colorScheme.surfaceContainerHighest;
        final align = isSelf ? CrossAxisAlignment.end : CrossAxisAlignment.start;
        final radius = BorderRadius.circular(16);
        return Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
          child: Column(
            crossAxisAlignment: align,
            children: [
              GestureDetector(
                onLongPress: () async {
                  await showModalBottomSheet<void>(
                    context: context,
                    builder: (context) => SafeArea(
                      child: Wrap(
                        children: [
                          ListTile(
                            leading: const Icon(Icons.emoji_emotions_outlined),
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
                  decoration: BoxDecoration(color: bubbleColor, borderRadius: radius),
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  child: Column(
                    crossAxisAlignment: align,
                    children: [
                      if (replyId != null && replyId.isNotEmpty)
                        Container(
                          margin: const EdgeInsets.only(bottom: 6),
                          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                          decoration: BoxDecoration(
                            color: Theme.of(context).colorScheme.surfaceContainerHighest,
                            borderRadius: BorderRadius.circular(8),
                          ),
                          child: Text(
                            replyMsg == null ? context.l10n.chatReplyUnknown : _replyPreview(replyMsg),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.labelSmall,
                          ),
                        ),
                      Text(text),
                      if (attachments.isNotEmpty) ...[
                        const SizedBox(height: 8),
                        _buildAttachments(attachments),
                      ],
                      const SizedBox(height: 4),
                      Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            formatTimeAgo(context, when),
                            style: Theme.of(context).textTheme.labelSmall,
                          ),
                          if (status != null) ...[
                            const SizedBox(width: 6),
                            Tooltip(
                              message: status.tooltip(context),
                              child: Icon(
                                status.icon,
                                size: 14,
                                color: Theme.of(context).colorScheme.onSurfaceVariant,
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
                  color: r.me ? Theme.of(context).colorScheme.primary.withAlpha(30) : bg,
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(
                    color: r.me ? Theme.of(context).colorScheme.primary : Colors.transparent,
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
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                      decoration: BoxDecoration(
                        color: Theme.of(context).colorScheme.surfaceContainerHighest,
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
                    onPressed: _sending ? null : () => setState(() => _replyTo = null),
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
                      label: Text(_media[i].name, maxLines: 1, overflow: TextOverflow.ellipsis),
                      onDeleted: _sending ? null : () => setState(() => _media.removeAt(i)),
                    ),
                ],
              ),
            ),
          Row(
            children: [
              IconButton(
                tooltip: context.l10n.composeAttachments,
                onPressed: _sending ? null : _pickFiles,
                icon: const Icon(Icons.attach_file),
              ),
              IconButton(
                tooltip: context.l10n.composeEmojiButton,
                onPressed: _sending ? null : _insertEmoji,
                icon: const Icon(Icons.emoji_emotions_outlined),
              ),
              IconButton(
                tooltip: context.l10n.chatGif,
                onPressed: _sending ? null : _openGifPicker,
                icon: const Icon(Icons.gif_box),
              ),
              Expanded(
                child: TextField(
                  controller: _composerCtrl,
                  minLines: 1,
                  maxLines: 4,
                  decoration: InputDecoration(
                    hintText: context.l10n.chatMessageHint,
                    border: OutlineInputBorder(borderRadius: BorderRadius.circular(16)),
                  ),
                  onSubmitted: (_) => _sendMessage(),
                ),
              ),
              const SizedBox(width: 8),
              IconButton(
                tooltip: context.l10n.chatSend,
                onPressed: _sending ? null : _sendMessage,
                icon: _sending
                    ? const SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.send),
              ),
            ],
          ),
        ],
      ),
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
    if (payload.attachments?.isNotEmpty ?? false) return context.l10n.chatReplyAttachment;
    return context.l10n.chatMessageEmpty;
  }

  ChatMessageItem? _findMessageById(String id) {
    for (final m in _messages) {
      if (m.messageId == id) return m;
    }
    return null;
  }

  Widget _buildAttachments(List<ChatAttachment> attachments) {
    final media = attachments.where((a) => _isMedia(a.mediaType, a.url)).toList();
    final docs = attachments.where((a) => !_isMedia(a.mediaType, a.url)).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
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
              title: Text(a.name ?? a.url, maxLines: 1, overflow: TextOverflow.ellipsis),
              onTap: () => openUrlExternal(a.url),
            ),
        ],
      ],
    );
  }

  bool _isMedia(String mediaType, String url) {
    final mt = mediaType.toLowerCase();
    if (mt.startsWith('image/') || mt.startsWith('video/') || mt.startsWith('audio/')) return true;
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

  void _openMediaViewer(List<ChatAttachment> attachments, ChatAttachment focus) {
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
    if (delivered > 0) return Icons.done;
    if (sent > 0) return Icons.check;
    if (queued > 0) return Icons.schedule;
    return Icons.schedule;
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
  final List<String> _suggestions = const ['funny', 'love', 'wow', 'party', 'sad', 'cat', 'dog'];

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
                        _search.selection = TextSelection.collapsed(offset: s.length);
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
                          gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
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
                                    color: Theme.of(context).colorScheme.surfaceContainerHighest,
                                    child: const Icon(Icons.broken_image_outlined),
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
