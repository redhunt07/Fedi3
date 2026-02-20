/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:file_selector/file_selector.dart';
import 'package:mime/mime.dart';
import 'package:http/http.dart' as http;

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/chat_models.dart';
import '../../model/core_config.dart';
import '../../model/ui_prefs.dart';
import '../../services/actor_repository.dart';
import '../../services/core_event_stream.dart';
import '../../services/gif_service.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
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
  bool _showArchived = false;
  StreamSubscription<CoreEvent>? _streamSub;
  Timer? _streamDebounce;
  Timer? _streamRetry;
  Timer? _loadRetry;
  Timer? _timeTicker;
  CoreConfig? _streamConfig;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadThreads(reset: true));
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
    _timeTicker = Timer.periodic(const Duration(minutes: 1), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _streamSub?.cancel();
    _streamDebounce?.cancel();
    _streamRetry?.cancel();
    _loadRetry?.cancel();
    _timeTicker?.cancel();
    widget.appState.removeListener(_appStateListener);
    super.dispose();
  }

  Future<void> _markSeen() async {
    final now = DateTime.now().millisecondsSinceEpoch;
    await widget.appState.savePrefs(widget.appState.prefs.copyWith(lastChatSeenMs: now));
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
      final resp = await api.fetchChatThreads(
        cursor: reset ? null : _next,
        limit: 50,
        archived: _showArchived,
      );
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
      builder: (_) => _NewChatDialog(config: cfg, prefs: widget.appState.prefs),
    );

    if (threadId != null && threadId.isNotEmpty && outerContext.mounted) {
      await _loadThreads(reset: true);
      ChatThreadItem? created;
      for (final t in _threads) {
        if (t.threadId == threadId) {
          created = t;
          break;
        }
      }
      final fallbackName = created?.title?.trim().isNotEmpty == true
          ? created!.title!.trim()
          : ((created?.kind ?? 'group') == 'dm'
              ? outerContext.l10n.chatDirectMessage
              : outerContext.l10n.chatGroup);
      final dmActor = created?.dmActor?.trim() ?? '';
      ActorProfile? profile;
      if (created?.kind == 'dm' && dmActor.isNotEmpty) {
        profile = await ActorRepository.instance.getActor(dmActor);
      }
      final resolvedName = (profile?.displayName.trim().isNotEmpty == true)
          ? profile!.displayName.trim()
          : ((created?.kind == 'dm')
              ? _dmFallbackName(dmActor, fallbackName)
              : fallbackName);
      await Navigator.of(outerContext).push(
        MaterialPageRoute(
          builder: (_) => ChatThreadScreen(
            appState: widget.appState,
            threadId: threadId,
            title: resolvedName,
            dmActor: dmActor.isNotEmpty ? dmActor : null,
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
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(44),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
            child: SegmentedButton<bool>(
              segments: [
                ButtonSegment(
                  value: false,
                  label: Text(context.l10n.chatThreadsActive),
                  icon: const Icon(Icons.forum_outlined),
                ),
                ButtonSegment(
                  value: true,
                  label: Text(context.l10n.chatThreadsArchived),
                  icon: const Icon(Icons.archive_outlined),
                ),
              ],
              selected: {_showArchived},
              onSelectionChanged: (value) {
                final next = value.contains(true);
                if (next == _showArchived) return;
                setState(() {
                  _showArchived = next;
                  _threads = const [];
                  _next = null;
                });
                _loadThreads(reset: true);
              },
            ),
          ),
        ),
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
      return ListView(children: [const SizedBox(height: 120), Center(child: Text(context.l10n.chatThreadsEmpty))]);
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
    return ListView.separated(
      itemCount: all.length + (_next != null ? 1 : 0),
      separatorBuilder: (_, __) => const Divider(height: 1),
      itemBuilder: (context, index) {
        if (index >= all.length) {
          return Padding(
            padding: const EdgeInsets.all(16),
            child: Center(
              child: OutlinedButton(
                onPressed: _loading ? null : () => _loadThreads(reset: false),
                child: Text(context.l10n.listLoadMore),
              ),
            ),
          );
        }
        final thread = all[index];
        final fallbackName = thread.title?.trim().isNotEmpty == true
            ? thread.title!.trim()
            : (thread.kind == 'dm' ? context.l10n.chatDirectMessage : context.l10n.chatGroup);
        final preview = thread.lastMessagePreview?.trim().isNotEmpty == true
            ? thread.lastMessagePreview!.trim()
            : context.l10n.chatNoMessages;
        final isGif = preview == 'GIF';
        final hasAttachment = preview == 'Attachment' || isGif;
        final ts = thread.lastMessageMs ?? thread.updatedAtMs;
        final when = ts > 0 ? DateTime.fromMillisecondsSinceEpoch(ts) : DateTime.now();
        final seenMs = widget.appState.prefs.chatThreadSeenMs[thread.threadId] ?? 0;
        final isUnread = ts > seenMs;
        final isPinned = pinnedIds.contains(thread.threadId);
        final dmActor = thread.dmActor?.trim() ?? '';
        return FutureBuilder<ActorProfile?>(
          future: (thread.kind == 'dm' && dmActor.isNotEmpty)
              ? ActorRepository.instance.getActor(dmActor)
              : Future<ActorProfile?>.value(null),
          builder: (context, snapshot) {
            final profile = snapshot.data;
            final resolvedName = (profile?.displayName.trim().isNotEmpty == true)
                ? profile!.displayName.trim()
                : (thread.kind == 'dm'
                    ? _dmFallbackName(dmActor, fallbackName)
                    : fallbackName);
            final leading = thread.kind == 'dm'
                ? _dmAvatar(profile, dmActor)
                : CircleAvatar(
                    radius: 20,
                    backgroundColor: Theme.of(context).colorScheme.surfaceContainerHighest,
                    child: const Icon(Icons.groups, size: 20),
                  );
            return ListTile(
              leading: leading,
              title: Row(
                children: [
                  if (isPinned) ...[
                    const Icon(Icons.push_pin, size: 16),
                    const SizedBox(width: 6),
                  ],
                  Expanded(child: Text(resolvedName, maxLines: 1, overflow: TextOverflow.ellipsis)),
                  if (isUnread)
                    Container(
                      width: 8,
                      height: 8,
                      margin: const EdgeInsets.only(left: 6),
                      decoration: BoxDecoration(
                        color: Theme.of(context).colorScheme.primary,
                        shape: BoxShape.circle,
                      ),
                    ),
                ],
              ),
              subtitle: Row(
                children: [
                  if (hasAttachment) ...[
                    Icon(isGif ? Icons.gif_box : Icons.attach_file, size: 14),
                    const SizedBox(width: 4),
                  ],
                  Expanded(
                    child: Text(
                      preview,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if (hasAttachment) ...[
                    const SizedBox(width: 6),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                      decoration: BoxDecoration(
                        color: Theme.of(context).colorScheme.primary.withAlpha(25),
                        borderRadius: BorderRadius.circular(10),
                      ),
                      child: Text(
                        isGif ? 'GIF' : context.l10n.chatReplyAttachment,
                        style: Theme.of(context).textTheme.labelSmall,
                      ),
                    ),
                  ],
                ],
              ),
              trailing: Text(formatTimeAgo(context, when)),
              onLongPress: () => _showThreadMenu(thread, isPinned),
              onTap: () async {
                await Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (_) => ChatThreadScreen(
                      appState: widget.appState,
                      threadId: thread.threadId,
                      title: resolvedName,
                      dmActor: dmActor,
                      isArchived: _showArchived,
                    ),
                  ),
                );
                if (mounted) {
                  await _loadThreads(reset: true);
                }
              },
            );
          },
        );
      },
    );
  }

  String _dmFallbackName(String actorId, String fallback) {
    final id = actorId.trim();
    if (id.isEmpty) return fallback;
    final uri = Uri.tryParse(id);
    if (uri == null) return fallback;
    final segs = uri.pathSegments;
    if (segs.length >= 2 && segs.first == 'users') {
      return segs[1];
    }
    return segs.isNotEmpty ? segs.last : fallback;
  }

  Widget _dmAvatar(ActorProfile? profile, String actorId) {
    final icon = profile?.iconUrl.trim() ?? '';
    return StatusAvatar(
      imageUrl: icon,
      size: 40,
      showStatus: true,
      statusKey: profile?.statusKey,
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
                leading: Icon(pinned ? Icons.push_pin_outlined : Icons.push_pin),
                title: Text(pinned ? context.l10n.chatUnpin : context.l10n.chatPin),
                onTap: () => Navigator.of(context).pop('pin'),
              ),
              ListTile(
                leading: Icon(_showArchived ? Icons.unarchive : Icons.archive),
                title: Text(_showArchived
                    ? context.l10n.chatUnarchiveThreadOption
                    : context.l10n.chatArchiveThreadOption),
                onTap: () => Navigator.of(context).pop(_showArchived ? 'unarchive' : 'archive'),
              ),
            ],
          ),
        );
      },
    );
    if (action == 'pin') {
      if (pinned) {
        nextPinned.removeWhere((id) => id == thread.threadId);
      } else {
        if (!nextPinned.contains(thread.threadId)) {
          nextPinned.add(thread.threadId);
        }
      }
      await widget.appState.savePrefs(prefs.copyWith(pinnedChatThreads: nextPinned));
      if (mounted) setState(() {});
      return;
    }
    if (action == 'archive' || action == 'unarchive') {
      final cfg = widget.appState.config;
      if (cfg == null) return;
      try {
        await CoreApi(config: cfg).archiveChatThread(
          threadId: thread.threadId,
          archived: action == 'archive',
        );
        if (mounted) {
          await _loadThreads(reset: true);
        }
      } catch (e) {
        if (!mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(e.toString())),
        );
      }
    }
  }
}

class _NewChatDialog extends StatefulWidget {
  const _NewChatDialog({required this.config, required this.prefs});

  final CoreConfig config;
  final UiPrefs prefs;

  @override
  State<_NewChatDialog> createState() => _NewChatDialogState();
}

class _NewChatDialogState extends State<_NewChatDialog> {
  final TextEditingController _recipientsCtrl = TextEditingController();
  final TextEditingController _titleCtrl = TextEditingController();
  final TextEditingController _messageCtrl = TextEditingController();
  String? _error;
  bool _sending = false;
  bool _searching = false;
  List<ActorProfile> _suggestions = const [];
  Timer? _searchDebounce;
  final List<_PickedMedia> _media = [];

  @override
  void dispose() {
    _searchDebounce?.cancel();
    _recipientsCtrl.dispose();
    _titleCtrl.dispose();
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
    if (recipientsRaw.isEmpty || (message.isEmpty && _media.isEmpty)) {
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
      final attachments = <Map<String, dynamic>>[];
      for (final m in _media) {
        if (m.coreMediaId == null || m.url == null || m.mediaType == null) {
          final resp = await api.uploadMedia(bytes: m.bytes, filename: m.name);
          final id = (resp['id'] as String?)?.trim();
          final url = (resp['url'] as String?)?.trim();
          final mediaType =
              (resp['media_type'] as String?)?.trim() ?? lookupMimeType(m.name) ?? '';
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
      final resp = await api.sendChatMessage(
        recipients: resolved,
        text: message,
        title: _titleCtrl.text.trim(),
        attachments: attachments,
      );
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
                    color: Theme.of(context).colorScheme.surfaceContainerHighest,
                  ),
                  child: ListView.separated(
                    shrinkWrap: true,
                    itemCount: _suggestions.length,
                    separatorBuilder: (_, __) => const Divider(height: 1),
                    itemBuilder: (context, index) {
                      final profile = _suggestions[index];
                      return ListTile(
                        dense: true,
                        title: Text(profile.displayName, maxLines: 1, overflow: TextOverflow.ellipsis),
                        subtitle: Text(profile.id, maxLines: 1, overflow: TextOverflow.ellipsis),
                        onTap: () {
                          final raw = _recipientsCtrl.text;
                          final parts = raw.split(RegExp(r'[,\n]'));
                          final last = parts.isNotEmpty ? parts.last : '';
                          final prefix = raw.substring(0, raw.length - last.length);
                          final next = '$prefix${profile.id}, ';
                          _recipientsCtrl.text = next;
                          _recipientsCtrl.selection = TextSelection.fromPosition(
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
              controller: _titleCtrl,
              decoration: InputDecoration(
                labelText: context.l10n.chatRename,
                hintText: context.l10n.chatRenameHint,
              ),
              textInputAction: TextInputAction.next,
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
            if (_media.isNotEmpty) ...[
              const SizedBox(height: 8),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final m in _media)
                    InputChip(
                      label: Text(m.name, overflow: TextOverflow.ellipsis),
                      onDeleted: _sending
                          ? null
                          : () {
                              setState(() => _media.remove(m));
                            },
                    ),
                ],
              ),
            ],
            const SizedBox(height: 8),
            Align(
              alignment: Alignment.centerLeft,
              child: Wrap(
                spacing: 8,
                children: [
                  TextButton.icon(
                    onPressed: _sending ? null : _pickFiles,
                    icon: const Icon(Icons.attach_file),
                    label: Text(context.l10n.chatReplyAttachment),
                  ),
                  TextButton.icon(
                    onPressed: _sending ? null : _openGifPicker,
                    icon: const Icon(Icons.gif_box),
                    label: Text(context.l10n.chatGif),
                  ),
                ],
              ),
            ),
            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
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
              ? const SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2))
              : Text(context.l10n.chatCreate),
        ),
      ],
    );
  }

  Future<void> _pickFiles() async {
    try {
      final files = await openFiles();
      for (final f in files) {
        final bytes = await f.readAsBytes();
        final name = f.name.isNotEmpty ? f.name : context.l10n.composeFileFallback;
        _media.add(_PickedMedia(name: name, bytes: bytes));
      }
      if (mounted) setState(() {});
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _openGifPicker() async {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (context) {
        return _GifPicker(
          prefs: widget.prefs,
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
}

class _PickedMedia {
  _PickedMedia({required this.name, required this.bytes});

  final String name;
  final Uint8List bytes;
  String? coreMediaId;
  String? url;
  String? mediaType;
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
