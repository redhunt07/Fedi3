/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/note_models.dart';
import '../../services/actor_repository.dart';
import '../../services/object_repository.dart';
import '../../services/translation_service.dart';
import '../../state/app_state.dart';
import '../screens/profile_screen.dart';
import '../screens/search_screen.dart';
import '../screens/note_detail_screen.dart';
import '../screens/media_viewer_screen.dart';
import '../utils/time_ago.dart';
import '../utils/media_url.dart';
import '../utils/open_url.dart';
import 'actor_hover_card.dart';
import 'inline_media_tile.dart';
import 'link_preview_card.dart';
import 'mention_pill.dart';
import 'reaction_picker.dart';
import 'status_avatar.dart';
import '../../state/reaction_store.dart';
import '../../state/emoji_store.dart';
import '../../state/note_flags_store.dart';

class NoteCard extends StatefulWidget {
  const NoteCard({
    super.key,
    required this.appState,
    required this.item,
    this.elevated = false,
    this.showRawFallback = false,
    this.rawActivity,
    this.autoExpandThread = false,
    this.autoExpandReplies = false,
  });

  final AppState appState;
  final TimelineItem item;
  final bool elevated;
  final bool showRawFallback;
  final Map<String, dynamic>? rawActivity;
  final bool autoExpandThread;
  final bool autoExpandReplies;

  @override
  State<NoteCard> createState() => _NoteCardState();
}

class _NoteCardState extends State<NoteCard> {
  ActorProfile? _noteActor;
  ActorProfile? _boostActor;
  bool _showRaw = false;
  bool _replying = false;
  final TextEditingController _replyCtrl = TextEditingController();
  bool _showThread = false;
  bool _threadLoading = false;
  String? _threadError;
  final List<Note> _threadParents = [];

  bool _showReplies = false;
  bool _repliesLoading = false;
  String? _repliesError;
  String? _repliesCursor;
  final List<Map<String, dynamic>> _replies = [];

  bool _cwOpen = false;
  Note? _overrideNote;
  bool _deletedOverride = false;

  String _resolveMediaUrl(String url) {
    return resolveLocalMediaUrl(widget.appState.config, url);
  }

  String _rawActivitySummary(Map<String, dynamic>? raw) {
    if (raw == null) return '';
    final type = (raw['type'] as String?)?.trim() ?? '';
    final actor = (raw['actor'] as String?)?.trim() ?? '';
    final obj = raw['object'];
    var objId = '';
    if (obj is String) {
      objId = obj.trim();
    } else if (obj is Map) {
      objId = (obj['id'] as String?)?.trim() ?? (obj['url'] as String?)?.trim() ?? '';
    }
    final parts = <String>[];
    if (type.isNotEmpty) parts.add(type);
    if (actor.isNotEmpty) parts.add(actor);
    if (objId.isNotEmpty && objId != actor) parts.add(objId);
    return parts.join(' • ');
  }

  bool _reactionsOpen = false;
  bool _reactionsLoading = false;
  String? _reactionsError;
  final List<Map<String, dynamic>> _reactionCounts = [];
  _ReactionStats? _liveStats;
  bool _myReactionsLoading = false;
  String? _myLikeId;
  String? _myAnnounceId;
  final Map<String, String> _myEmojiIds = {};
  bool _bookmarked = false;
  bool _pinned = false;
  bool _muted = false;
  bool _translationLoading = false;
  bool _translationVisible = false;
  String? _translatedText;
  String? _translationSource;
  String? _translationError;

  @override
  void initState() {
    super.initState();
    _showThread = widget.autoExpandThread;
    _showReplies = widget.autoExpandReplies;
    _loadActors();
    _liveStats = _statsFromRaw(widget.rawActivity);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final cfg = widget.appState.config;
      if (cfg == null) return;
      final api = CoreApi(config: cfg);
      final note = widget.item.note;
      EmojiStore.addAll(note.emojis);
      _loadFlags(note.id);
      if (_showThread && note.inReplyTo.trim().isNotEmpty && _threadParents.isEmpty && !_threadLoading) {
        _loadThreadParents(api, note.inReplyTo);
      }
      if (_showReplies && note.id.trim().isNotEmpty && _replies.isEmpty && !_repliesLoading && widget.appState.isRunning) {
        _loadReplies(api, note.id);
      }
      if (widget.appState.isRunning) {
        _loadMyReactions(api, note.id);
      }
    });
  }

  @override
  void didUpdateWidget(covariant NoteCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.rawActivity != widget.rawActivity) {
      _liveStats = _statsFromRaw(widget.rawActivity);
    }
    if (oldWidget.item.note.id != widget.item.note.id) {
      _overrideNote = null;
      _deletedOverride = false;
      _myLikeId = null;
      _myAnnounceId = null;
      _myEmojiIds.clear();
      _bookmarked = false;
      _pinned = false;
      _muted = false;
      _translationLoading = false;
      _translationVisible = false;
      _translatedText = null;
      _translationSource = null;
      _translationError = null;
      final cfg = widget.appState.config;
      if (cfg != null && widget.appState.isRunning) {
        _loadMyReactions(CoreApi(config: cfg), widget.item.note.id);
      }
      EmojiStore.addAll(widget.item.note.emojis);
      _loadFlags(widget.item.note.id);
    }
  }

  @override
  void dispose() {
    _replyCtrl.dispose();
    super.dispose();
  }

  Future<void> _loadActors() async {
    final noteActor = widget.item.note.attributedTo.trim();
    if (noteActor.isNotEmpty) {
      final p = await ActorRepository.instance.getActor(noteActor);
      if (mounted) setState(() => _noteActor = p);
    }
    final booster = widget.item.boostedBy.trim();
    if (booster.isNotEmpty && booster != noteActor) {
      final p = await ActorRepository.instance.getActor(booster);
      if (mounted) setState(() => _boostActor = p);
    }
  }

  void _openHashtag(BuildContext context, String tag) {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => SearchScreen(
          appState: widget.appState,
          initialQuery: '#$tag',
          initialTab: SearchTab.posts,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config!;
    final api = CoreApi(config: cfg);
    final canAct = widget.appState.isRunning;

    final note = _overrideNote ?? widget.item.note;
    final isDeleted = _deletedOverride;
    final isOwner = _isOwner(note);
    final stats = _liveStats ?? _statsFromRaw(widget.rawActivity);
    final content = _injectCustomEmoji(note.contentHtml, note.emojis);
    final previewUrl = _extractFirstLinkPreviewUrl(note.contentHtml, note.attachments);
    final display = _noteActor?.displayName ?? _fallbackActorLabel(note.attributedTo);
    final handle = _handleFor(note);
    final shouldCw = note.sensitive || note.summary.isNotEmpty;
    final cwTitle = note.summary.isNotEmpty ? note.summary : context.l10n.noteContentWarning;

    final borderColor = Theme.of(context).colorScheme.outlineVariant.withAlpha(110);
    return Card(
      elevation: widget.elevated ? 2 : 0,
      margin: EdgeInsets.zero,
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        borderRadius: BorderRadius.circular(14),
        onTap: widget.rawActivity == null
            ? null
            : () {
                Navigator.of(context).push(
                  MaterialPageRoute(builder: (_) => NoteDetailScreen(appState: widget.appState, activity: widget.rawActivity!)),
                );
              },
        child: Container(
          decoration: BoxDecoration(
            border: _borderForActivity(context),
          ),
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (_boostActor != null)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 8),
                    child: Row(
                      children: [
                        const Icon(Icons.repeat, size: 16),
                        const SizedBox(width: 6),
                        Expanded(
                          child: Text(
                            context.l10n.noteBoostedBy(_boostActor!.displayName),
                            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12),
                          ),
                        ),
                      ],
                    ),
                  ),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  _Avatar(
                    url: _noteActor?.iconUrl ?? '',
                    size: 40,
                    showStatus: _noteActor?.isFedi3 == true,
                    statusKey: _noteActor?.statusKey,
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Expanded(
                              child: ActorHoverCard(
                                actorUrl: note.attributedTo,
                                onTap: () => _openProfile(context, note.attributedTo),
                                child: Text.rich(
                                  TextSpan(
                                    children: [
                                      TextSpan(
                                        text: display,
                                        style: const TextStyle(fontWeight: FontWeight.w800),
                                      ),
                                      TextSpan(
                                        text: handle.isNotEmpty ? ' · $handle' : '',
                                        style: TextStyle(
                                          color: Theme.of(context).colorScheme.onSurface.withAlpha(170),
                                          fontSize: 12,
                                        ),
                                      ),
                                    ],
                                  ),
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                            ),
                            const SizedBox(width: 8),
                            Text(
                              _formatPublished(context, note.published),
                              style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
                            ),
                            if (canAct && isOwner && !isDeleted)
                              PopupMenuButton<String>(
                                tooltip: context.l10n.noteActions,
                                onSelected: (value) {
                                  if (value == 'edit') {
                                    _editNote(context, api, note);
                                  } else if (value == 'delete') {
                                    _deleteNote(context, api, note);
                                  }
                                },
                                itemBuilder: (context) => [
                                  PopupMenuItem(value: 'edit', child: Text(context.l10n.noteEdit)),
                                  PopupMenuItem(value: 'delete', child: Text(context.l10n.noteDelete)),
                                ],
                              ),
                          ],
                        ),
                        Row(
                          children: [
                            if (widget.item.activityType == 'Announce')
                              _HeaderBadge(
                                label: context.l10n.notificationsBoost,
                                color: Theme.of(context).colorScheme.primary.withAlpha(180),
                                icon: Icons.repeat,
                              ),
                            if (note.inReplyTo.isNotEmpty)
                              Padding(
                                padding: EdgeInsets.only(left: widget.item.activityType == 'Announce' ? 6 : 0),
                                child: _HeaderBadge(
                                  label: context.l10n.noteInReplyTo,
                                  color: Theme.of(context).colorScheme.secondary.withAlpha(180),
                                  icon: Icons.reply,
                                ),
                              ),
                          ],
                        ),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              if (isDeleted)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8),
                  child: Text(
                    context.l10n.noteDeleted,
                    style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
                  ),
                )
              else ...[
                if (note.inReplyTo.isNotEmpty && (widget.item.inReplyToPreview != null || widget.item.quotePreview != null))
                  _QuotedPreview(
                    appState: widget.appState,
                    label: context.l10n.noteInReplyTo,
                    inReplyTo: note.inReplyTo,
                    replyPreview: widget.item.inReplyToPreview,
                    quotePreview: widget.item.quotePreview,
                  ),
                if (shouldCw)
                  _ContentWarning(
                    titleHtml: _injectCustomEmoji(cwTitle, note.emojis),
                    open: _cwOpen,
                    onToggle: () => setState(() => _cwOpen = !_cwOpen),
                    child: _NoteContent(
                      appState: widget.appState,
                      html: content,
                      onTapUrl: (url) async {
                        if (_looksLikeActorUrl(url)) {
                          _openProfile(context, url);
                          return true;
                        }
                        return openUrlExternal(url);
                      },
                    ),
                  )
                else if (content.isNotEmpty)
                  _NoteContent(
                    appState: widget.appState,
                    html: content,
                    onTapUrl: (url) async {
                      if (_looksLikeActorUrl(url)) {
                        _openProfile(context, url);
                        return true;
                      }
                      return openUrlExternal(url);
                    },
                  )
                else if (widget.showRawFallback)
                  Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(context.l10n.activityUnsupported),
                      Builder(
                        builder: (context) {
                          final summary = _rawActivitySummary(widget.rawActivity);
                          if (summary.isEmpty) return const SizedBox.shrink();
                          return Padding(
                            padding: const EdgeInsets.only(top: 4),
                            child: Text(
                              summary,
                              style: TextStyle(
                                fontSize: 12,
                                color: Theme.of(context).colorScheme.onSurface.withAlpha(170),
                              ),
                            ),
                          );
                        },
                      ),
                    ],
                  )
                else
                  const SizedBox.shrink(),
              ],
              if (note.hashtags.isNotEmpty) ...[
                const SizedBox(height: 6),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    for (final tag in note.hashtags)
                      ActionChip(
                        label: Text('#$tag'),
                        onPressed: () => _openHashtag(context, tag),
                      ),
                  ],
                ),
              ],
              if (_translationVisible && _translatedText != null) ...[
                const SizedBox(height: 8),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(140),
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(120)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      if (_translationSource != null && _translationSource!.isNotEmpty)
                        Text(
                          context.l10n.noteTranslatedFrom(_translationSource!),
                          style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160), fontSize: 12),
                        ),
                      if (_translationSource != null && _translationSource!.isNotEmpty) const SizedBox(height: 6),
                      Text(_translatedText!, style: const TextStyle(fontSize: 14)),
                      const SizedBox(height: 6),
                      Align(
                        alignment: Alignment.centerRight,
                        child: TextButton(
                          onPressed: () => setState(() => _translationVisible = false),
                          child: Text(context.l10n.noteShowOriginal),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
              if (_translationError != null && !_translationVisible) ...[
                const SizedBox(height: 8),
                Text(
                  context.l10n.noteTranslateFailed(_translationError!),
                  style: TextStyle(color: Theme.of(context).colorScheme.error, fontSize: 12),
                ),
              ],
              if (stats != null) ...[
                const SizedBox(height: 6),
                _StatsRow(
                  stats: stats,
                  noteEmojis: note.emojis,
                  onTap: canAct ? () => _showReactionsPopup(context, api, note.id) : null,
                ),
              ],
              if (previewUrl != null) ...[
                const SizedBox(height: 8),
                LinkPreviewCard(url: previewUrl),
              ],
              if (note.attachments.isNotEmpty) ...[
                const SizedBox(height: 8),
                _AttachmentsGrid(
                  attachments: note.attachments,
                  onOpen: (index) {
                    final safe = index.clamp(0, note.attachments.length - 1);
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => MediaViewerScreen(
                          appState: widget.appState,
                          url: note.attachments[safe].url,
                          mediaType: note.attachments[safe].mediaType,
                          attachments: note.attachments,
                          initialIndex: safe,
                        ),
                      ),
                    );
                  },
                  autoplay: false,
                  muted: true,
                  resolveUrl: _resolveMediaUrl,
                ),
              ],
              if (_muted) ...[
                const SizedBox(height: 8),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120),
                    borderRadius: BorderRadius.circular(12),
                  ),
                  child: Row(
                    children: [
                      const Icon(Icons.volume_off, size: 16),
                      const SizedBox(width: 8),
                      const Expanded(child: Text('Thread muted', style: TextStyle(fontWeight: FontWeight.w700))),
                      TextButton(
                        onPressed: () => _toggleFlag(context, 'mute', note.id),
                        child: const Text('Unmute'),
                      ),
                    ],
                  ),
                ),
              ] else ...[
                if (note.inReplyTo.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  _InlineThread(
                    appState: widget.appState,
                    note: note,
                    show: _showThread,
                    loading: _threadLoading,
                    error: _threadError,
                    parents: _threadParents,
                    onToggle: () async {
                      final next = !_showThread;
                      setState(() => _showThread = next);
                      if (next && _threadParents.isEmpty && !_threadLoading) {
                        await _loadThreadParents(api, note.inReplyTo);
                      }
                    },
                  ),
                ],
                const SizedBox(height: 8),
                _InlineReplies(
                  appState: widget.appState,
                  noteId: note.id,
                  show: _showReplies,
                  loading: _repliesLoading,
                  error: _repliesError,
                  replies: _replies,
                  onToggle: () async {
                    final next = !_showReplies;
                    setState(() => _showReplies = next);
                    if (next && _replies.isEmpty && !_repliesLoading && canAct) {
                      await _loadReplies(api, note.id);
                    }
                  },
                  onLoadMore: (_repliesCursor == null || _repliesLoading || !canAct)
                      ? null
                      : () async => _loadReplies(api, note.id, cursor: _repliesCursor),
                ),
              ],
              const SizedBox(height: 8),
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Wrap(
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      _ActionButton(
                        tooltip: context.l10n.activityReply,
                        icon: const Icon(Icons.reply, size: 18),
                        onPressed: canAct
                            ? () => setState(() {
                                  _replying = !_replying;
                                  if (!_replying) _replyCtrl.clear();
                                })
                            : null,
                      ),
                      _ActionButton(
                        tooltip: context.l10n.activityBoost,
                        icon: Icon(Icons.repeat, size: 18, color: _myAnnounceId != null ? Theme.of(context).colorScheme.primary : null),
                        count: stats?.boostCount ?? 0,
                        onPressed: canAct ? () => _toggleBoost(context, api, note.id, note.attributedTo) : null,
                      ),
                      _ActionButton(
                        tooltip: context.l10n.activityLike,
                        icon: Icon(
                          _myLikeId != null ? Icons.favorite : Icons.favorite_border,
                          size: 18,
                          color: _myLikeId != null ? Theme.of(context).colorScheme.primary : null,
                        ),
                        count: stats?.likeCount ?? 0,
                        onPressed: (canAct && note.attributedTo.isNotEmpty) ? () => _toggleLike(context, api, note.id, note.attributedTo) : null,
                      ),
                      _ActionButton(
                        tooltip: context.l10n.activityReact,
                        icon: Icon(
                          Icons.add_reaction_outlined,
                          size: 18,
                          color: _myEmojiIds.isNotEmpty ? Theme.of(context).colorScheme.primary : null,
                        ),
                        count: stats == null ? 0 : stats.emojis.fold(0, (sum, e) => sum + e.count),
                        onPressed: (canAct && note.attributedTo.isNotEmpty)
                            ? () async {
                                final next = !_reactionsOpen;
                                setState(() => _reactionsOpen = next);
                                if (next && _reactionCounts.isEmpty && !_reactionsLoading) {
                                  await _loadReactions(api, note.id);
                                }
                              }
                            : null,
                      ),
                      _ActionButton(
                        tooltip: _translatedText == null
                            ? context.l10n.noteTranslate
                            : (_translationVisible ? context.l10n.noteShowOriginal : context.l10n.noteShowTranslation),
                        icon: _translationLoading
                            ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                            : Icon(
                                Icons.translate,
                                size: 18,
                                color: _translationVisible ? Theme.of(context).colorScheme.primary : null,
                              ),
                        onPressed: _translationLoading ? null : () => _toggleTranslate(context, note.contentHtml),
                      ),
                      _ActionButton(
                        tooltip: 'Bookmark',
                        icon: Icon(_bookmarked ? Icons.bookmark : Icons.bookmark_border, size: 18),
                        onPressed: () => _toggleFlag(context, 'bookmark', note.id),
                      ),
                      _ActionButton(
                        tooltip: 'Pin',
                        icon: Icon(_pinned ? Icons.push_pin : Icons.push_pin_outlined, size: 18),
                        onPressed: () => _toggleFlag(context, 'pin', note.id),
                      ),
                      _ActionButton(
                        tooltip: _muted ? 'Unmute thread' : 'Mute thread',
                        icon: Icon(_muted ? Icons.volume_off : Icons.volume_up, size: 18),
                        onPressed: () => _toggleFlag(context, 'mute', note.id),
                      ),
                      _ActionButton(
                        tooltip: 'Copy link',
                        icon: const Icon(Icons.link, size: 18),
                        onPressed: () async {
                          await Clipboard.setData(ClipboardData(text: note.id));
                          if (!context.mounted) return;
                          ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Link copied')));
                        },
                      ),
                    ],
                  ),
                  if (note.inReplyTo.isNotEmpty || widget.item.activityType == 'Announce') ...[
                    const SizedBox(height: 6),
                    Align(
                      alignment: Alignment.centerRight,
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                        decoration: BoxDecoration(
                          color: Theme.of(context).colorScheme.surfaceContainerHigh.withAlpha(120),
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: borderColor),
                        ),
                        child: Text(
                          widget.item.activityType == 'Announce'
                              ? context.l10n.notificationsBoost
                              : context.l10n.noteInReplyTo,
                          style: TextStyle(
                            fontSize: 11,
                            color: Theme.of(context).colorScheme.onSurface.withAlpha(160),
                          ),
                        ),
                      ),
                    ),
                  ],
                  if (widget.showRawFallback && widget.rawActivity != null)
                    Align(
                      alignment: Alignment.centerRight,
                      child: TextButton(
                        onPressed: () => setState(() => _showRaw = !_showRaw),
                        child: Text(_showRaw ? context.l10n.activityHideRaw : context.l10n.activityViewRaw),
                      ),
                    ),
                ],
              ),
              if (_reactionsOpen) ...[
                const SizedBox(height: 8),
                _ReactionsPanel(
                  loading: _reactionsLoading,
                  error: _reactionsError,
                  counts: _reactionCounts,
                  myEmojis: _myEmojiIds.keys.toSet(),
                  onToggle: (canAct && note.attributedTo.isNotEmpty)
                      ? (emoji) async => _toggleEmoji(context, api, note.id, note.attributedTo, emoji)
                      : null,
                  onAdd: (canAct && note.attributedTo.isNotEmpty)
                      ? () async {
                          final picked = await _pickReaction(context);
                          if (picked == null) return;
                          if (!context.mounted) return;
                          await _toggleEmoji(context, api, note.id, note.attributedTo, picked, forceAdd: true);
                        }
                      : null,
                ),
              ],
              if (_replying) ...[
                const SizedBox(height: 8),
                TextField(
                  controller: _replyCtrl,
                  minLines: 1,
                  maxLines: 5,
                  decoration: InputDecoration(hintText: context.l10n.activityReplyHint),
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    TextButton(
                      onPressed: () => setState(() {
                        _replying = false;
                        _replyCtrl.clear();
                      }),
                      child: Text(context.l10n.activityCancel),
                    ),
                    const Spacer(),
                    FilledButton(
                      onPressed: () => _sendInlineReply(context, api, note.id, note.attributedTo),
                      child: Text(context.l10n.activitySend),
                    ),
                  ],
                ),
              ],
              if (_showRaw && widget.rawActivity != null)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text(
                    const JsonEncoder.withIndent('  ').convert(widget.rawActivity),
                    style: TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 12,
                      color: Theme.of(context).colorScheme.onSurface.withAlpha(204),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _toggleTranslate(BuildContext context, String html) async {
    if (_translationLoading) return;
    if (_translatedText != null) {
      setState(() => _translationVisible = !_translationVisible);
      return;
    }
    final plain = _plainTextFromHtml(html);
    if (plain.isEmpty) return;
    setState(() {
      _translationLoading = true;
      _translationError = null;
    });
    try {
      final prefs = widget.appState.prefs;
      final target = _targetLangFromContext(context, prefs.localeTag);
      final res = await TranslationService.translate(text: plain, targetLang: target, prefs: prefs);
      if (!mounted) return;
      setState(() {
        _translatedText = res.text;
        _translationSource = res.detectedSource;
        _translationVisible = true;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _translationError = e.toString());
    } finally {
      if (mounted) setState(() => _translationLoading = false);
    }
  }

  String _targetLangFromContext(BuildContext context, String override) {
    final tag = override.trim();
    if (tag.isNotEmpty) return tag;
    return Localizations.localeOf(context).languageCode;
  }

  String _plainTextFromHtml(String html) {
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

  String _htmlFromPlain(String text) {
    final escaped = _escape(text);
    final withBreaks = escaped.replaceAll('\n', '<br>');
    return '<p>$withBreaks</p>';
  }

  String _injectCustomEmoji(String html, List<NoteEmoji> emojis) {
    var out = html;
    for (final e in emojis) {
      final code = e.name.trim();
      if (code.isEmpty) continue;
      final img = '<img alt="${_escape(code)}" src="${_escape(e.iconUrl)}" style="height: 1em; vertical-align: -0.15em;" />';
      out = out.replaceAll(code, img);
    }
    return out;
  }

  String _escape(String s) => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');

  BoxBorder? _borderForActivity(BuildContext context) {
    final type = widget.item.activityType;
    if (type != 'Announce' && widget.item.note.inReplyTo.trim().isEmpty) return null;
    final scheme = Theme.of(context).colorScheme;
    final color = type == 'Announce'
        ? scheme.primary.withAlpha(160)
        : scheme.secondary.withAlpha(160);
    return Border(left: BorderSide(color: color, width: 3));
  }

  bool _looksLikeActorUrl(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return false;
    return uri.path.startsWith('/users/') || uri.path.startsWith('/@');
  }

  bool _isOwner(Note note) {
    final cfg = widget.appState.config;
    if (cfg == null) return false;
    final base = cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final me = '$base/users/${cfg.username.trim()}';
    return note.attributedTo == me;
  }

  Map<String, dynamic>? _extractNoteObject(Map<String, dynamic> activity) {
    final obj = activity['object'];
    if (obj is Map) {
      final map = obj.cast<String, dynamic>();
      final ty = (map['type'] as String?)?.trim() ?? '';
      if (ty == 'Note' || ty == 'Article' || ty == 'Question') return map;
      final inner = map['object'];
      if (inner is Map) {
        final innerMap = inner.cast<String, dynamic>();
        final innerType = (innerMap['type'] as String?)?.trim() ?? '';
        if (innerType == 'Note' || innerType == 'Article' || innerType == 'Question') {
          return innerMap;
        }
      }
    }
    return null;
  }

  List<String> _extractAudience(dynamic value) {
    if (value is String) {
      final v = value.trim();
      return v.isEmpty ? const [] : [v];
    }
    if (value is List) {
      return value.whereType<String>().map((v) => v.trim()).where((v) => v.isNotEmpty).toList();
    }
    return const [];
  }

  List<dynamic>? _attachmentsForUpdate(Note note, Map<String, dynamic>? objectMap) {
    final raw = objectMap?['attachment'];
    if (raw is List && raw.isNotEmpty) {
      return raw;
    }
    if (note.attachments.isEmpty) return null;
    return note.attachments
        .map((a) => {
              'url': a.url,
              'mediaType': a.mediaType,
            })
        .toList();
  }

  Future<_NoteAudience?> _resolveAudience(CoreApi api, Note note) async {
    final to = <String>[];
    final cc = <String>[];
    var objType = 'Note';
    Map<String, dynamic>? objectMap;

    final raw = widget.rawActivity;
    if (raw != null) {
      to.addAll(_extractAudience(raw['to']));
      cc.addAll(_extractAudience(raw['cc']));
      objectMap = _extractNoteObject(raw);
      if (objectMap != null) {
        if (to.isEmpty) to.addAll(_extractAudience(objectMap['to']));
        if (cc.isEmpty) cc.addAll(_extractAudience(objectMap['cc']));
        final ty = (objectMap['type'] as String?)?.trim();
        if (ty != null && ty.isNotEmpty) objType = ty;
      }
    }

    if (to.isEmpty && cc.isEmpty) {
      objectMap ??= await api.fetchCachedObject(note.id);
      if (objectMap != null) {
        to.addAll(_extractAudience(objectMap['to']));
        cc.addAll(_extractAudience(objectMap['cc']));
        final ty = (objectMap['type'] as String?)?.trim();
        if (ty != null && ty.isNotEmpty) objType = ty;
      }
    }

    final mergedTo = to.where((v) => v.isNotEmpty).toSet().toList();
    final mergedCc = cc.where((v) => v.isNotEmpty).toSet().toList();
    if (mergedTo.isEmpty && mergedCc.isEmpty) {
      return null;
    }
    return _NoteAudience(
      to: mergedTo,
      cc: mergedCc,
      objectType: objType,
      objectMap: objectMap,
    );
  }

  void _openProfile(BuildContext context, String actorUrl) {
    Navigator.of(context).push(
      MaterialPageRoute(builder: (_) => ProfileScreen(appState: widget.appState, actorUrl: actorUrl)),
    );
  }

  Future<void> _editNote(BuildContext context, CoreApi api, Note note) async {
    final contentCtrl = TextEditingController(text: _plainTextFromHtml(note.contentHtml));
    final summaryCtrl = TextEditingController(text: _plainTextFromHtml(note.summary));
    var sensitive = note.sensitive;
    final result = await showDialog<bool>(
      context: context,
      builder: (context) {
        return StatefulBuilder(
          builder: (context, setState) {
            return AlertDialog(
              title: Text(context.l10n.noteEditTitle),
              content: SizedBox(
                width: 420,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    TextField(
                      controller: contentCtrl,
                      minLines: 3,
                      maxLines: 8,
                      decoration: InputDecoration(hintText: context.l10n.noteEditContentHint),
                    ),
                    const SizedBox(height: 12),
                    TextField(
                      controller: summaryCtrl,
                      minLines: 1,
                      maxLines: 3,
                      decoration: InputDecoration(hintText: context.l10n.noteEditSummaryHint),
                    ),
                    CheckboxListTile(
                      value: sensitive,
                      onChanged: (value) => setState(() => sensitive = value ?? false),
                      title: Text(context.l10n.noteSensitiveLabel),
                      controlAffinity: ListTileControlAffinity.leading,
                      contentPadding: EdgeInsets.zero,
                    ),
                  ],
                ),
              ),
              actions: [
                TextButton(onPressed: () => Navigator.of(context).pop(false), child: Text(context.l10n.activityCancel)),
                FilledButton(
                  onPressed: () => Navigator.of(context).pop(true),
                  child: Text(context.l10n.noteEditSave),
                ),
              ],
            );
          },
        );
      },
    );
    if (result != true) {
      contentCtrl.dispose();
      summaryCtrl.dispose();
      return;
    }
    final content = contentCtrl.text.trim();
    final summary = summaryCtrl.text.trim();
    contentCtrl.dispose();
    summaryCtrl.dispose();
    if (content.isEmpty) return;
    final audience = await _resolveAudience(api, note);
    if (audience == null) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.noteEditMissingAudience)));
      return;
    }
    try {
      await api.editNote(
        objectId: note.id,
        content: content,
        to: audience.to,
        cc: audience.cc,
        summary: summary.isEmpty ? null : summary,
        sensitive: sensitive || summary.isNotEmpty,
        inReplyTo: note.inReplyTo,
        attachments: _attachmentsForUpdate(note, audience.objectMap),
      );
      if (!mounted) return;
      setState(() {
        _overrideNote = Note(
          id: note.id,
          attributedTo: note.attributedTo,
          contentHtml: _htmlFromPlain(content),
          summary: summary.isEmpty ? note.summary : _htmlFromPlain(summary),
          sensitive: sensitive || summary.isNotEmpty,
          published: note.published,
          inReplyTo: note.inReplyTo,
          attachments: note.attachments,
          emojis: note.emojis,
          hashtags: note.hashtags,
        );
      });
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    }
  }

  Future<void> _deleteNote(BuildContext context, CoreApi api, Note note) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.noteDeleteTitle),
        content: Text(context.l10n.noteDeleteHint),
        actions: [
          TextButton(onPressed: () => Navigator.of(context).pop(false), child: Text(context.l10n.activityCancel)),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.noteDeleteConfirm),
          ),
        ],
      ),
    );
    if (ok != true) return;
    final audience = await _resolveAudience(api, note);
    if (audience == null) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.noteDeleteMissingAudience)));
      return;
    }
    try {
      await api.deleteNote(
        objectId: note.id,
        to: audience.to,
        cc: audience.cc,
        objectType: audience.objectType,
      );
      if (!mounted) return;
      setState(() {
        _deletedOverride = true;
        _myLikeId = null;
        _myAnnounceId = null;
        _myEmojiIds.clear();
        _reactionsOpen = false;
        _replying = false;
        _replyCtrl.clear();
      });
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    }
  }

  Future<void> _loadMyReactions(CoreApi api, String objectId) async {
    if (_myReactionsLoading) return;
    final oid = objectId.trim();
    if (oid.isEmpty) return;
    _myReactionsLoading = true;
    try {
      final resp = await api.fetchMyReactions(oid);
      if (!mounted) return;
      final likeId = (resp['like'] as String?)?.trim();
      final announceId = (resp['announce'] as String?)?.trim();
      final emojis = <String, String>{};
      final list = resp['emojis'];
      if (list is List) {
        for (final it in list) {
          if (it is! Map) continue;
          final m = it.cast<String, dynamic>();
          final id = (m['id'] as String?)?.trim();
          final content = (m['content'] as String?)?.trim();
          if (id == null || id.isEmpty || content == null || content.isEmpty) continue;
          emojis[content] = id;
        }
      }
      setState(() {
        _myLikeId = likeId != null && likeId.isNotEmpty ? likeId : null;
        _myAnnounceId = announceId != null && announceId.isNotEmpty ? announceId : null;
        _myEmojiIds
          ..clear()
          ..addAll(emojis);
      });
    } catch (_) {
      // Best-effort: ignore if unavailable.
    } finally {
      _myReactionsLoading = false;
    }
  }

  Future<void> _loadFlags(String noteId) async {
    final id = noteId.trim();
    if (id.isEmpty) return;
    final bookmarked = await NoteFlagsStore.contains('bookmark', id);
    final pinned = await NoteFlagsStore.contains('pin', id);
    final muted = await NoteFlagsStore.contains('mute', id);
    if (!mounted) return;
    setState(() {
      _bookmarked = bookmarked;
      _pinned = pinned;
      _muted = muted;
    });
  }

  Future<void> _toggleFlag(BuildContext context, String key, String noteId) async {
    final id = noteId.trim();
    if (id.isEmpty) return;
    final next = await NoteFlagsStore.toggle(key, id);
    if (key == 'pin') {
      final cfg = widget.appState.config;
      if (cfg != null) {
        try {
          final api = CoreApi(config: cfg);
          await api.setNotePinned(noteId: id, pinned: next);
        } catch (_) {
          // Best-effort: keep local flag even if server fails.
        }
      }
    }
    if (!mounted) return;
    setState(() {
      if (key == 'bookmark') _bookmarked = next;
      if (key == 'pin') _pinned = next;
      if (key == 'mute') _muted = next;
      if (key == 'mute' && next) {
        _showReplies = false;
        _replies.clear();
      }
    });
    if (!context.mounted) return;
    final label = switch (key) {
      'bookmark' => next ? 'Bookmarked' : 'Bookmark removed',
      'pin' => next ? 'Pinned' : 'Pin removed',
      'mute' => next ? 'Thread muted' : 'Thread unmuted',
      _ => next ? 'Updated' : 'Updated',
    };
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(label)));
  }

  Future<void> _toggleLike(BuildContext context, CoreApi api, String objectId, String objectActor) async {
    if (_myLikeId != null) {
      final prev = _myLikeId;
      setState(() => _myLikeId = null);
      _applyLocalDelta(likeDelta: -1);
      try {
        await api.undoReaction(innerType: 'Like', objectId: objectId, objectActor: objectActor, innerId: prev);
      } catch (e) {
        if (mounted) setState(() => _myLikeId = prev);
        _applyLocalDelta(likeDelta: 1);
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
        return;
      }
    } else {
      _applyLocalDelta(likeDelta: 1);
      try {
        await api.like(objectId: objectId, objectActor: objectActor);
      } catch (e) {
        _applyLocalDelta(likeDelta: -1);
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
        return;
      }
    }
    await _loadMyReactions(api, objectId);
  }

  Future<void> _toggleBoost(BuildContext context, CoreApi api, String objectId, String objectActor) async {
    if (_myAnnounceId != null) {
      final prev = _myAnnounceId;
      setState(() => _myAnnounceId = null);
      _applyLocalDelta(boostDelta: -1);
      try {
        await api.undoReaction(innerType: 'Announce', objectId: objectId, objectActor: objectActor, innerId: prev);
      } catch (e) {
        if (mounted) setState(() => _myAnnounceId = prev);
        _applyLocalDelta(boostDelta: 1);
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
        return;
      }
    } else {
      _applyLocalDelta(boostDelta: 1);
      try {
        await api.boost(objectId: objectId, public: true);
      } catch (e) {
        _applyLocalDelta(boostDelta: -1);
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
        return;
      }
    }
    await _loadMyReactions(api, objectId);
  }

  Future<void> _toggleEmoji(
    BuildContext context,
    CoreApi api,
    String objectId,
    String objectActor,
    String emoji, {
    bool forceAdd = false,
  }) async {
    final em = emoji.trim();
    if (em.isEmpty) return;
    final existing = _myEmojiIds[em];
    if (existing != null && !forceAdd) {
      _myEmojiIds.remove(em);
      _applyLocalDelta(emoji: em, emojiDelta: -1);
      try {
        await api.undoReaction(innerType: 'EmojiReact', objectId: objectId, objectActor: objectActor, innerId: existing, content: em);
      } catch (e) {
        _myEmojiIds[em] = existing;
        _applyLocalDelta(emoji: em, emojiDelta: 1);
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
        return;
      }
    } else {
      _applyLocalDelta(emoji: em, emojiDelta: 1);
      try {
        await api.react(objectId: objectId, objectActor: objectActor, emoji: em);
        await ReactionStore.add(em);
      } catch (e) {
        _applyLocalDelta(emoji: em, emojiDelta: -1);
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
        return;
      }
    }
    await _loadReactions(api, objectId);
    await _loadMyReactions(api, objectId);
  }

  Future<void> _showReactionsPopup(BuildContext context, CoreApi api, String objectId) async {
    if (_reactionCounts.isEmpty && !_reactionsLoading) {
      await _loadReactions(api, objectId);
    }
    if (!context.mounted) return;
    showDialog(
      context: context,
      builder: (context) {
        if (_reactionCounts.isEmpty) {
          return AlertDialog(
            title: Text(context.l10n.noteReactionAdd),
            content: Text(context.l10n.listNoItems),
            actions: [
              TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(context.l10n.activityCancel)),
            ],
          );
        }
        return AlertDialog(
          title: Text(context.l10n.noteReactionAdd),
          content: SizedBox(
            width: 320,
            child: ListView(
              shrinkWrap: true,
              children: [
                for (final r in _reactionCounts)
                  ListTile(
                    dense: true,
                    leading: Text(
                      (r['content'] as String?)?.trim().isNotEmpty == true ? (r['content'] as String) : _fallbackEmojiForType(r['type']?.toString() ?? ''),
                    ),
                    title: Text('${(r['count'] is num) ? (r['count'] as num).toInt() : int.tryParse(r['count']?.toString() ?? '') ?? 0}'),
                    trailing: TextButton(
                      onPressed: () {
                        final ty = (r['type'] as String?)?.trim() ?? '';
                        final content = (r['content'] as String?)?.trim();
                        if (ty.isEmpty) return;
                        Navigator.of(context).pop();
                        _showReactionActorsDialog(context, api, objectId, ty, content);
                      },
                      child: const Text('Users'),
                    ),
                  ),
              ],
            ),
          ),
          actions: [
            TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(context.l10n.activityCancel)),
          ],
        );
      },
    );
  }

  String _fallbackEmojiForType(String ty) {
    if (ty == 'Like') return '❤️';
    if (ty == 'Announce') return '🔁';
    return '⭐';
  }

  Future<void> _showReactionActorsDialog(
    BuildContext context,
    CoreApi api,
    String objectId,
    String type,
    String? content,
  ) async {
    final label = (content != null && content.trim().isNotEmpty) ? content.trim() : _fallbackEmojiForType(type);
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: Text('Reactions $label'),
          content: SizedBox(
            width: 320,
            child: FutureBuilder<List<String>>(
              future: api.fetchReactionActors(objectId: objectId, type: type, content: content),
              builder: (context, snap) {
                if (snap.connectionState == ConnectionState.waiting) {
                  return const Padding(
                    padding: EdgeInsets.symmetric(vertical: 12),
                    child: Center(child: CircularProgressIndicator()),
                  );
                }
                if (snap.hasError) {
                  return Text('${snap.error}', style: TextStyle(color: Theme.of(context).colorScheme.error));
                }
                final items = snap.data ?? const [];
                if (items.isEmpty) return Text(context.l10n.listNoItems);
                return ListView(
                  shrinkWrap: true,
                  children: [
                    for (final url in items)
                      FutureBuilder<ActorProfile?>(
                        future: ActorRepository.instance.getActor(url),
                        builder: (context, actorSnap) {
                          final profile = actorSnap.data;
                          final name = profile?.displayName.trim().isNotEmpty == true ? profile!.displayName : url;
                          return ListTile(
                            dense: true,
                            leading: _Avatar(
                              url: profile?.iconUrl ?? '',
                              size: 28,
                              showStatus: profile?.isFedi3 == true,
                              statusKey: profile?.statusKey,
                            ),
                            title: Text(name, overflow: TextOverflow.ellipsis),
                            subtitle: Text(url, overflow: TextOverflow.ellipsis),
                            onTap: () => _openProfile(context, url),
                          );
                        },
                      ),
                  ],
                );
              },
            ),
          ),
          actions: [
            TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(context.l10n.activityCancel)),
          ],
        );
      },
    );
  }

  Future<void> _sendInlineReply(BuildContext context, CoreApi api, String inReplyTo, String replyToActor) async {
    final body = _replyCtrl.text.trim();
    if (body.isEmpty) return;
    try {
      await api.postNote(content: body, public: true, mediaIds: const [], inReplyTo: inReplyTo, replyToActor: replyToActor);
      if (!context.mounted) return;
      setState(() {
        _replying = false;
        _replyCtrl.clear();
      });
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    }
  }

  Future<void> _loadThreadParents(CoreApi api, String start) async {
    final first = start.trim();
    if (first.isEmpty) return;
    setState(() {
      _threadLoading = true;
      _threadError = null;
      _threadParents.clear();
    });
    try {
      var cur = first;
      for (var i = 0; i < 3; i++) {
        Map<String, dynamic>? obj = await api.fetchCachedObject(cur);
        obj ??= await ObjectRepository.instance.fetchObject(cur);
        if (obj == null) break;
        final n = Note.tryParse(obj);
        if (n == null) break;
        _threadParents.add(n);
        cur = n.inReplyTo.trim();
        if (cur.isEmpty) break;
      }
    } catch (e) {
      _threadError = e.toString();
    } finally {
      if (mounted) setState(() => _threadLoading = false);
    }
  }

  Future<void> _loadReactions(CoreApi api, String objectId) async {
    final oid = objectId.trim();
    if (oid.isEmpty) return;
    setState(() {
      _reactionsLoading = true;
      _reactionsError = null;
    });
    try {
      final resp = await api.fetchReactions(oid, limit: 50);
      final items = (resp['items'] as List<dynamic>? ?? const [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .toList();
      if (!mounted) return;
      setState(() {
        _reactionCounts
          ..clear()
          ..addAll(items);
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _reactionsError = e.toString());
    } finally {
      if (mounted) setState(() => _reactionsLoading = false);
    }
  }

  Future<String?> _pickReaction(BuildContext context) async {
    final note = widget.item.note;
    final picked = await ReactionPicker.show(
      context,
      noteEmojis: note.emojis,
      prefs: widget.appState.prefs,
    );
    if (picked == null) return null;
    await ReactionStore.add(picked);
    return picked;
  }

  void _applyLocalDelta({int likeDelta = 0, int boostDelta = 0, String? emoji, int emojiDelta = 0}) {
    if (!mounted) return;
    final base = _liveStats ?? _statsFromRaw(widget.rawActivity) ?? const _ReactionStats(likeCount: 0, boostCount: 0, emojis: []);
    final likes = (base.likeCount + likeDelta).clamp(0, 1 << 30);
    final boosts = (base.boostCount + boostDelta).clamp(0, 1 << 30);

    final nextEmojis = base.emojis.map((e) => _EmojiCount(emoji: e.emoji, count: e.count)).toList();
    final em = (emoji ?? '').trim();
    if (em.isNotEmpty && emojiDelta != 0) {
      final idx = nextEmojis.indexWhere((e) => e.emoji == em);
      if (idx >= 0) {
        final updated = (nextEmojis[idx].count + emojiDelta).clamp(0, 1 << 30);
        nextEmojis[idx] = _EmojiCount(emoji: em, count: updated);
      } else if (emojiDelta > 0) {
        nextEmojis.insert(0, _EmojiCount(emoji: em, count: emojiDelta));
      }
      nextEmojis.removeWhere((e) => e.count <= 0);
      nextEmojis.sort((a, b) => b.count.compareTo(a.count));
    }

    setState(() {
      _liveStats = _ReactionStats(likeCount: likes, boostCount: boosts, emojis: nextEmojis);
    });
  }

  Future<void> _loadReplies(CoreApi api, String noteId, {String? cursor}) async {
    final nid = noteId.trim();
    if (nid.isEmpty) return;
    setState(() {
      _repliesLoading = true;
      _repliesError = null;
    });
    try {
      final resp = await api.fetchNoteReplies(nid, cursor: cursor, limit: 20);
      final items = (resp['items'] as List<dynamic>? ?? const [])
          .whereType<Map>()
          .map((m) => m.cast<String, dynamic>())
          .toList();
      final next = (resp['next'] as String?)?.trim();
      if (!mounted) return;
      setState(() {
        _replies.addAll(items);
        _repliesCursor = (next != null && next.isNotEmpty) ? next : null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _repliesError = e.toString());
    } finally {
      if (mounted) setState(() => _repliesLoading = false);
    }
  }

  String _formatPublished(BuildContext context, String published) {
    final s = published.trim();
    if (s.isEmpty) return '';
    final dt = DateTime.tryParse(s);
    if (dt == null) return '';
    return formatTimeAgo(context, dt.toLocal());
  }

  String _fallbackActorLabel(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null) return url;
    final host = uri.host;
    if (uri.pathSegments.isNotEmpty && uri.pathSegments.first == 'users' && uri.pathSegments.length >= 2) {
      return '${uri.pathSegments[1]}@$host';
    }
    return host;
  }

  String _handleFor(Note note) {
    final url = note.attributedTo.trim();
    final uri = Uri.tryParse(url);
    final host = uri?.host ?? '';
    final username = _noteActor?.preferredUsername.trim() ?? '';
    if (username.isNotEmpty && host.isNotEmpty) return '@$username@$host';
    if (username.isNotEmpty) return '@$username';
    if (host.isNotEmpty && uri != null) {
      final segs = uri.pathSegments;
      if (segs.isNotEmpty && segs.first == 'users' && segs.length >= 2) {
        return '@${segs[1]}@$host';
      }
      return '@$host';
    }
    return url;
  }

  String? _extractFirstLinkPreviewUrl(String html, List<NoteAttachment> attachments) {
    final h = _decodeHtmlEntities(html.trim());
    if (h.isEmpty) return null;

    final attachmentUrls = attachments.map((a) => a.url.trim()).where((s) => s.isNotEmpty).toSet();

    String? pick(String candidate) {
      final u = _trimUrlPunct(candidate.trim());
      if (!(u.startsWith('http://') || u.startsWith('https://'))) return null;
      final uri = Uri.tryParse(u);
      if (uri == null || uri.host.isEmpty) return null;
      final path = uri.path;
      if (path.startsWith('/users/') || path.startsWith('/@')) return null;
      if (attachmentUrls.contains(u)) return null;
      return u;
    }

    // Prefer <a href="..."> URLs.
    final hrefRe = RegExp("href=[\"'](https?://[^\"'\\s<>]+)[\"']", caseSensitive: false);
    for (final m in hrefRe.allMatches(h)) {
      final v = m.group(1);
      if (v == null) continue;
      final p = pick(v);
      if (p != null) return p;
    }

    // Fallback: raw URL text in HTML.
    final urlRe = RegExp(r'(https?://[^\\s<>\"\\)\\]]+)', caseSensitive: false);
    for (final m in urlRe.allMatches(h)) {
      final v = m.group(1);
      if (v == null) continue;
      final p = pick(v);
      if (p != null) return p;
    }
    return null;
  }

  String _decodeHtmlEntities(String s) {
    return s
        .replaceAll('&amp;', '&')
        .replaceAll('&quot;', '"')
        .replaceAll('&apos;', "'")
        .replaceAll('&#39;', "'")
        .replaceAll('&#039;', "'")
        .replaceAll('&lt;', '<')
        .replaceAll('&gt;', '>');
  }

  String _trimUrlPunct(String url) {
    var out = url;
    const trailing = '.,;:!?)"]}';
    while (out.isNotEmpty && trailing.contains(out[out.length - 1])) {
      out = out.substring(0, out.length - 1);
    }
    return out;
  }
}

class _ReactionStats {
  const _ReactionStats({
    required this.likeCount,
    required this.boostCount,
    required this.emojis,
  });

  final int likeCount;
  final int boostCount;
  final List<_EmojiCount> emojis;
}

class _EmojiCount {
  const _EmojiCount({required this.emoji, required this.count});
  final String emoji;
  final int count;
}

_ReactionStats? _statsFromRaw(Map<String, dynamic>? activity) {
  final a = activity;
  if (a == null) return null;
  final list = a['fedi3ReactionCounts'];
  if (list is! List) return null;

  var likes = 0;
  var boosts = 0;
  final emojis = <_EmojiCount>[];

  for (final it in list) {
    if (it is! Map) continue;
    final m = it.cast<String, dynamic>();
    final ty = (m['type'] as String?)?.trim() ?? '';
    final count = (m['count'] is num) ? (m['count'] as num).toInt() : int.tryParse(m['count']?.toString() ?? '') ?? 0;
    if (count <= 0) continue;

    if (ty == 'Like') {
      likes += count;
      continue;
    }
    if (ty == 'Announce') {
      boosts += count;
      continue;
    }
    if (ty == 'EmojiReact') {
      final e = (m['content'] as String?)?.trim() ?? '';
      if (e.isEmpty) continue;
      emojis.add(_EmojiCount(emoji: e, count: count));
    }
  }

  if (likes == 0 && boosts == 0 && emojis.isEmpty) return null;
  return _ReactionStats(likeCount: likes, boostCount: boosts, emojis: emojis);
}

class _StatsRow extends StatelessWidget {
  const _StatsRow({required this.stats, required this.noteEmojis, required this.onTap});

  final _ReactionStats stats;
  final List<NoteEmoji> noteEmojis;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final fg = theme.colorScheme.onSurface.withAlpha(179);

    Widget counter(IconData icon, int count) {
      if (count <= 0) return const SizedBox.shrink();
      return Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        decoration: BoxDecoration(
          color: theme.colorScheme.surfaceContainerHighest.withAlpha(110),
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: theme.colorScheme.outlineVariant.withAlpha(120)),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 14, color: fg),
            const SizedBox(width: 4),
            Text('$count', style: TextStyle(color: fg, fontSize: 12, fontWeight: FontWeight.w700)),
          ],
        ),
      );
    }

    final emojiChips = stats.emojis.take(6).toList(growable: false);
    final emojiMap = <String, String>{
      for (final e in noteEmojis) e.name.trim(): e.iconUrl.trim(),
    };

    final row = Wrap(
      spacing: 8,
      runSpacing: 6,
      children: [
        counter(Icons.repeat, stats.boostCount),
        counter(Icons.favorite, stats.likeCount),
        for (final e in emojiChips)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            decoration: BoxDecoration(
              color: theme.colorScheme.surfaceContainerHighest.withAlpha(130),
              borderRadius: BorderRadius.circular(10),
              border: Border.all(color: theme.colorScheme.outlineVariant.withAlpha(120)),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _EmojiOrImage(code: e.emoji, emojiMap: emojiMap),
                const SizedBox(width: 6),
                Text('${e.count}', style: TextStyle(color: fg, fontSize: 12, fontWeight: FontWeight.w700)),
              ],
            ),
          ),
      ],
    );
    if (onTap == null) return row;
    return InkWell(
      borderRadius: BorderRadius.circular(10),
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: row,
      ),
    );
  }
}

class _EmojiOrImage extends StatelessWidget {
  const _EmojiOrImage({required this.code, required this.emojiMap});

  final String code;
  final Map<String, String> emojiMap;

  @override
  Widget build(BuildContext context) {
    final c = code.trim();
    final url = emojiMap[c];
    if (url == null || url.isEmpty) {
      return Text(c, style: const TextStyle(fontSize: 14));
    }
    return Image.network(
      url,
      width: 16,
      height: 16,
      fit: BoxFit.contain,
      errorBuilder: (_, __, ___) => Text(c, style: const TextStyle(fontSize: 14)),
    );
  }
}

class _ActionButton extends StatelessWidget {
  const _ActionButton({
    required this.tooltip,
    required this.icon,
    this.onPressed,
    this.count = 0,
  });

  final String tooltip;
  final Widget icon;
  final VoidCallback? onPressed;
  final int count;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Tooltip(
      message: tooltip,
      child: TextButton(
        onPressed: onPressed,
        style: TextButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
          minimumSize: Size.zero,
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          visualDensity: VisualDensity.compact,
          foregroundColor: theme.colorScheme.onSurface.withAlpha(200),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            icon,
            if (count > 0) ...[
              const SizedBox(width: 4),
              Text(
                '$count',
                style: TextStyle(fontSize: 12, color: theme.colorScheme.onSurface.withAlpha(190)),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _HeaderBadge extends StatelessWidget {
  const _HeaderBadge({
    required this.label,
    required this.color,
    required this.icon,
  });

  final String label;
  final Color color;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      decoration: BoxDecoration(
        color: color.withAlpha(50),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: color.withAlpha(120)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 12, color: color),
          const SizedBox(width: 4),
          Text(
            label,
            style: TextStyle(fontSize: 11, color: color, fontWeight: FontWeight.w700),
          ),
        ],
      ),
    );
  }
}

class _Avatar extends StatelessWidget {
  const _Avatar({
    required this.url,
    required this.size,
    this.showStatus = false,
    this.statusKey,
  });

  final String url;
  final double size;
  final bool showStatus;
  final String? statusKey;

  @override
  Widget build(BuildContext context) {
    return StatusAvatar(
      imageUrl: url,
      size: size,
      showStatus: showStatus,
      statusKey: statusKey,
    );
  }
}

class _AttachmentsGrid extends StatelessWidget {
  const _AttachmentsGrid({
    required this.attachments,
    required this.onOpen,
    required this.autoplay,
    required this.muted,
    required this.resolveUrl,
  });

  final List<NoteAttachment> attachments;
  final void Function(int index) onOpen;
  final bool autoplay;
  final bool muted;
  final String Function(String url) resolveUrl;

  @override
  Widget build(BuildContext context) {
    final items = attachments.where((a) => a.url.trim().startsWith('http')).toList();
    if (items.isEmpty) return const SizedBox.shrink();
    final count = items.length.clamp(1, 4);
    final display = items.take(count).toList();
    final dpr = MediaQuery.of(context).devicePixelRatio;

    if (count == 1) {
      final a = display.first;
      return InlineMediaTile(
        url: resolveUrl(a.url),
        mediaType: a.mediaType,
        cacheWidth: (700 * dpr).round(),
        borderRadius: 12,
        onOpen: () => onOpen(attachments.indexOf(a)),
        autoplay: autoplay,
        muted: muted,
      );
    }

    return GridView.count(
      crossAxisCount: 2,
      mainAxisSpacing: 6,
      crossAxisSpacing: 6,
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      children: [
        for (var i = 0; i < display.length; i++)
          Stack(
            fit: StackFit.expand,
            children: [
              InlineMediaTile(
                url: resolveUrl(display[i].url),
                mediaType: display[i].mediaType,
                cacheWidth: (350 * dpr).round(),
                borderRadius: 12,
                onOpen: () => onOpen(attachments.indexOf(display[i])),
                autoplay: autoplay,
                muted: muted,
              ),
              if (i == display.length - 1 && items.length > display.length)
                Container(
                  decoration: BoxDecoration(
                    color: Colors.black.withAlpha(120),
                    borderRadius: BorderRadius.circular(12),
                  ),
                  alignment: Alignment.center,
                  child: Text(
                    '+${items.length - display.length}',
                    style: const TextStyle(fontWeight: FontWeight.w800, fontSize: 18),
                  ),
                ),
            ],
          ),
      ],
    );
  }
}

class _QuotedPreview extends StatelessWidget {
  const _QuotedPreview({
    required this.appState,
    required this.label,
    required this.inReplyTo,
    required this.replyPreview,
    required this.quotePreview,
  });

  final AppState appState;
  final String label;
  final String inReplyTo;
  final Map<String, dynamic>? replyPreview;
  final Map<String, dynamic>? quotePreview;

  @override
  Widget build(BuildContext context) {
    final prev = replyPreview ?? quotePreview;
    if (prev == null) return const SizedBox.shrink();
    final inner = (prev['content'] as String?)?.trim() ?? (prev['name'] as String?)?.trim() ?? '';
    if (inner.isEmpty) return const SizedBox.shrink();

    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: () async {
          if (inReplyTo.isEmpty) return;
          final obj = await ObjectRepository.instance.fetchObject(inReplyTo);
          if (!context.mounted || obj == null) return;
          final note = Note.tryParse(obj);
          if (note == null) return;
          final activity = {
            'id': note.id,
            'type': 'Create',
            'actor': note.attributedTo,
            'object': obj,
          };
          Navigator.of(context).push(
            MaterialPageRoute(builder: (_) => NoteDetailScreen(appState: appState, activity: activity)),
          );
        },
        child: Container(
          decoration: BoxDecoration(
            color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(130),
            borderRadius: BorderRadius.circular(12),
          ),
          padding: const EdgeInsets.all(10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(label, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12)),
              const SizedBox(height: 6),
              HtmlWidget(
                inner,
                buildAsync: !Platform.isLinux,
                enableCaching: !Platform.isLinux,
              ),
              if (inReplyTo.isNotEmpty) ...[
                const SizedBox(height: 6),
                Text(
                  inReplyTo,
                  style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
                  overflow: TextOverflow.ellipsis,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _NoteContent extends StatelessWidget {
  const _NoteContent({required this.appState, required this.html, required this.onTapUrl});

  final AppState appState;
  final String html;
  final FutureOr<bool> Function(String url) onTapUrl;

  @override
  Widget build(BuildContext context) {
    final allowAsync = !Platform.isLinux;
    final content = _linkifyPlainUrls(html);
    return HtmlWidget(
      content,
      buildAsync: allowAsync,
      enableCaching: allowAsync,
      onTapUrl: onTapUrl,
      customWidgetBuilder: (element) {
        if (element.localName != 'a') return null;
        final href = element.attributes['href']?.trim() ?? '';
        if (href.isEmpty) return null;
        final isMention = element.classes.contains('mention') || element.classes.contains('u-url');
        if (!isMention) return null;
        // Prefer "mention-like" text (e.g. @user@host)
        final label = element.text.trim();
        if (label.isEmpty || !label.startsWith('@')) return null;
        return Padding(
          padding: const EdgeInsets.only(right: 4, bottom: 2),
          child: MentionPill(appState: appState, actorUrl: href, label: label),
        );
      },
    );
  }

  String _linkifyPlainUrls(String input) {
    final text = input.trim();
    if (text.isEmpty) return input;
    if (text.contains('<a ')) return input;
    final urlRe = RegExp(r'((?:https?://|www\.)[^\s<>"\]]+)', caseSensitive: false);
    return text.replaceAllMapped(urlRe, (m) {
      var raw = m.group(1) ?? '';
      if (raw.isEmpty) return m[0] ?? '';
      final trimmed = _trimUrlPunct(raw);
      final trailing = raw.substring(trimmed.length);
      final href = trimmed.startsWith('http') ? trimmed : 'https://$trimmed';
      return '<a href="$href">$trimmed</a>$trailing';
    });
  }

  String _trimUrlPunct(String url) {
    var out = url;
    const trailing = '.,;:!?)"]}';
    while (out.isNotEmpty && trailing.contains(out[out.length - 1])) {
      out = out.substring(0, out.length - 1);
    }
    return out;
  }
}

class _ContentWarning extends StatelessWidget {
  const _ContentWarning({
    required this.titleHtml,
    required this.open,
    required this.onToggle,
    required this.child,
  });

  final String titleHtml;
  final bool open;
  final VoidCallback onToggle;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(90),
        borderRadius: BorderRadius.circular(12),
      ),
      padding: const EdgeInsets.all(10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: HtmlWidget(
                  titleHtml,
                  buildAsync: !Platform.isLinux,
                  enableCaching: !Platform.isLinux,
                ),
              ),
              TextButton(
                onPressed: onToggle,
                child: Text(open ? context.l10n.noteHideContent : context.l10n.noteShowContent),
              ),
            ],
          ),
          if (open) ...[
            const SizedBox(height: 8),
            child,
          ],
        ],
      ),
    );
  }
}

class _ReactionsPanel extends StatelessWidget {
  const _ReactionsPanel({
    required this.loading,
    required this.error,
    required this.counts,
    required this.onAdd,
    required this.myEmojis,
    required this.onToggle,
  });

  final bool loading;
  final String? error;
  final List<Map<String, dynamic>> counts;
  final VoidCallback? onAdd;
  final Set<String> myEmojis;
  final void Function(String emoji)? onToggle;

  @override
  Widget build(BuildContext context) {
    if (loading) {
      return Text(context.l10n.noteReactionLoading);
    }
    if (error != null) {
      return Text(error!, style: TextStyle(color: Theme.of(context).colorScheme.error));
    }
    return Row(
      children: [
        Expanded(
          child: Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final r in counts.take(16))
                _ReactionChip(
                  emoji: (r['content'] as String?)?.trim().isNotEmpty == true ? (r['content'] as String) : _fallbackEmoji(r['type']?.toString() ?? ''),
                  count: (r['count'] is num) ? (r['count'] as num).toInt() : int.tryParse(r['count']?.toString() ?? '') ?? 0,
                  selected: myEmojis.contains((r['content'] as String?)?.trim() ?? ''),
                  onTap: (r['type']?.toString() == 'EmojiReact') ? onToggle : null,
                ),
            ],
          ),
        ),
        const SizedBox(width: 10),
        FilledButton(
          onPressed: onAdd,
          child: Text(context.l10n.noteReactionAdd),
        ),
      ],
    );
  }

  String _fallbackEmoji(String ty) {
    if (ty == 'Like') return '❤️';
    if (ty == 'Announce') return '🔁';
    return '⭐';
  }
}

class _ReactionChip extends StatelessWidget {
  const _ReactionChip({
    required this.emoji,
    required this.count,
    required this.selected,
    required this.onTap,
  });

  final String emoji;
  final int count;
  final bool selected;
  final void Function(String emoji)? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      decoration: BoxDecoration(
        color: selected ? theme.colorScheme.primary.withAlpha(30) : theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(999),
        border: selected ? Border.all(color: theme.colorScheme.primary.withAlpha(120)) : null,
      ),
      child: InkWell(
        borderRadius: BorderRadius.circular(999),
        onTap: onTap == null ? null : () => onTap!(emoji),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(emoji),
              const SizedBox(width: 6),
              Text('$count', style: const TextStyle(fontWeight: FontWeight.w700)),
            ],
          ),
        ),
      ),
    );
  }
}

class _InlineThread extends StatelessWidget {
  const _InlineThread({
    required this.appState,
    required this.note,
    required this.show,
    required this.loading,
    required this.error,
    required this.parents,
    required this.onToggle,
  });

  final AppState appState;
  final Note note;
  final bool show;
  final bool loading;
  final String? error;
  final List<Note> parents;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final line = Theme.of(context).dividerColor.withAlpha(120);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                context.l10n.noteInReplyTo,
                style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            TextButton(
              onPressed: onToggle,
              child: Text(show ? context.l10n.noteHideThread : context.l10n.noteShowThread),
            ),
          ],
        ),
        if (!show)
          Padding(
            padding: const EdgeInsets.only(left: 10),
            child: Text(
              note.inReplyTo,
              style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
              overflow: TextOverflow.ellipsis,
            ),
          ),
        if (show) ...[
          if (loading)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(context.l10n.noteLoadingThread),
            ),
          if (error != null)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error, fontSize: 12),
              ),
            ),
          if (!loading && parents.isEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6, left: 10),
              child: Text(
                note.inReplyTo,
                style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
                overflow: TextOverflow.ellipsis,
              ),
            ),
          if (parents.isNotEmpty) ...[
            const SizedBox(height: 6),
            for (final p in parents.reversed)
              _Nest(
                lineColor: line,
                child: _ThreadNotePreview(appState: appState, note: p),
              ),
          ],
        ],
      ],
    );
  }
}

class _ThreadNotePreview extends StatefulWidget {
  const _ThreadNotePreview({required this.appState, required this.note});

  final AppState appState;
  final Note note;

  @override
  State<_ThreadNotePreview> createState() => _ThreadNotePreviewState();
}

class _InlineReplies extends StatelessWidget {
  const _InlineReplies({
    required this.appState,
    required this.noteId,
    required this.show,
    required this.loading,
    required this.error,
    required this.replies,
    required this.onToggle,
    required this.onLoadMore,
  });

  final AppState appState;
  final String noteId;
  final bool show;
  final bool loading;
  final String? error;
  final List<Map<String, dynamic>> replies;
  final VoidCallback onToggle;
  final VoidCallback? onLoadMore;

  @override
  Widget build(BuildContext context) {
    final line = Theme.of(context).dividerColor.withAlpha(120);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                show ? context.l10n.noteHideReplies : context.l10n.noteShowReplies,
                style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 12),
              ),
            ),
            TextButton(
              onPressed: onToggle,
              child: Text(show ? context.l10n.noteHideReplies : context.l10n.noteShowReplies),
            ),
          ],
        ),
        if (show) ...[
          if (loading)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(context.l10n.noteLoadingReplies),
            ),
          if (error != null)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error, fontSize: 12),
              ),
            ),
          if (!loading && replies.isEmpty && error == null)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                context.l10n.listNoItems,
                style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
              ),
            ),
          if (replies.isNotEmpty) ...[
            for (final a in replies.take(10))
              _Nest(
                lineColor: line,
                child: Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: NoteCard(
                    appState: appState,
                    item: TimelineItem.tryFromActivity(a) ??
                        TimelineItem(
                          activityId: (a['id'] as String?)?.trim() ?? '',
                          activityType: (a['type'] as String?)?.trim() ?? '',
                          actor: (a['actor'] as String?)?.trim() ?? '',
                          note: Note(
                            id: noteId,
                            attributedTo: '',
                            contentHtml: '',
                            summary: '',
                            sensitive: false,
                            published: '',
                            inReplyTo: '',
                            attachments: const [],
                            emojis: const [],
                            hashtags: const [],
                          ),
                          boostedBy: '',
                          inReplyToPreview: null,
                          quotePreview: null,
                        ),
                    showRawFallback: true,
                    rawActivity: a,
                  ),
                ),
              ),
            if (onLoadMore != null)
              Align(
                alignment: Alignment.centerRight,
                child: TextButton(
                  onPressed: onLoadMore,
                  child: Text(context.l10n.listLoadMore),
                ),
              ),
          ],
        ],
      ],
    );
  }
}

class _Nest extends StatelessWidget {
  const _Nest({required this.child, required this.lineColor});

  final Widget child;
  final Color lineColor;

  @override
  Widget build(BuildContext context) {
    return IntrinsicHeight(
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const SizedBox(width: 10),
          VerticalDivider(width: 2, thickness: 2, color: lineColor),
          const SizedBox(width: 10),
          Expanded(child: child),
        ],
      ),
    );
  }
}

class _ThreadNotePreviewState extends State<_ThreadNotePreview> {
  ActorProfile? _actor;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final url = widget.note.attributedTo.trim();
    if (url.isEmpty) return;
    final p = await ActorRepository.instance.getActor(url);
    if (!mounted) return;
    setState(() => _actor = p);
  }

  @override
  Widget build(BuildContext context) {
    final name = _actor?.displayName ?? widget.note.attributedTo;
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _Avatar(
            url: _actor?.iconUrl ?? '',
            size: 28,
            showStatus: _actor?.isFedi3 == true,
            statusKey: _actor?.statusKey,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(name, style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 12), overflow: TextOverflow.ellipsis),
                const SizedBox(height: 2),
                _NoteContent(
                  appState: widget.appState,
                  html: widget.note.contentHtml,
                  onTapUrl: (url) async {
                    if (_looksLikeActorUrlLite(url)) {
                      Navigator.of(context).push(MaterialPageRoute(builder: (_) => ProfileScreen(appState: widget.appState, actorUrl: url)));
                      return true;
                    }
                    return false;
                  },
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  bool _looksLikeActorUrlLite(String url) {
    final u = url.trim();
    if (!u.startsWith('http://') && !u.startsWith('https://')) return false;
    final uri = Uri.tryParse(u);
    if (uri == null || uri.host.isEmpty) return false;
    final p = uri.path;
    return p.startsWith('/users/') || p.startsWith('/@');
  }
}

class _NoteAudience {
  const _NoteAudience({
    required this.to,
    required this.cc,
    required this.objectType,
    required this.objectMap,
  });

  final List<String> to;
  final List<String> cc;
  final String objectType;
  final Map<String, dynamic>? objectMap;
}
