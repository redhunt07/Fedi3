/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';
import 'package:mime/mime.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../../state/emoji_recent_store.dart';
import '../../state/direct_recipient_store.dart';
import '../widgets/emoji_picker.dart';
import '../widgets/mfm_cheatsheet.dart';
import '../../utils/mfm_codec.dart';
import 'status_avatar.dart';

class InlineComposer extends StatefulWidget {
  const InlineComposer({
    super.key,
    required this.appState,
    required this.api,
    required this.onPosted,
  });

  final AppState appState;
  final CoreApi api;
  final VoidCallback onPosted;

  @override
  State<InlineComposer> createState() => _InlineComposerState();
}

class _InlineComposerState extends State<InlineComposer> {
  static const maxChars = 7000;

  final _text = TextEditingController();
  final _cw = TextEditingController();
  String _visibility = 'public';
  final _directTo = TextEditingController();
  List<String> _directRecent = const [];
  bool _cwEnabled = false;
  bool _sensitive = false;
  bool _posting = false;
  String? _status;
  final _media = <_PickedMedia>[];
  bool _dragging = false;
  ActorProfile? _me;
  bool _showPreview = false;
  Timer? _mentionDebounce;
  bool _mentionSearching = false;
  List<ActorProfile> _mentionSuggestions = const [];
  int? _mentionStart;
  String _mentionQuery = '';

  @override
  void dispose() {
    _mentionDebounce?.cancel();
    _text.dispose();
    _cw.dispose();
    _directTo.dispose();
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    _loadMe();
    _loadDirectRecent();
  }

  Future<void> _loadMe() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final base = cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
    final user = cfg.username.trim();
    if (base.isEmpty || user.isEmpty) return;
    final actorUrl = '$base/users/$user';
    final p = await ActorRepository.instance.getActor(actorUrl);
    if (!mounted) return;
    setState(() => _me = p);
  }

  Future<void> _loadDirectRecent() async {
    final recent = await DirectRecipientStore.read();
    if (!mounted) return;
    setState(() => _directRecent = recent);
  }

  @override
  Widget build(BuildContext context) {
    final used = _text.text.characters.length;
    return Card(
      child: DropTarget(
        onDragEntered: (_) => setState(() => _dragging = true),
        onDragExited: (_) => setState(() => _dragging = false),
        onDragDone: (detail) async {
          setState(() => _dragging = false);
          await _addMediaFiles(detail.files);
        },
        child: Stack(
          children: [
            Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _TinyAvatar(
                        url: _me?.iconUrl ?? '',
                        size: 36,
                        showStatus: _me?.isFedi3 == true,
                        statusKey: _me?.statusKey,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Row(
                              children: [
                                PopupMenuButton<String>(
                                  tooltip: '${context.l10n.composeVisibilityTitle}: ${_visibilityLabel(context)}',
                                  onSelected: (v) => setState(() => _visibility = v),
                                  itemBuilder: (context) => [
                                    PopupMenuItem(value: 'public', child: Text(context.l10n.composeVisibilityPublic)),
                                    PopupMenuItem(value: 'home', child: Text(context.l10n.composeVisibilityHome)),
                                    PopupMenuItem(value: 'followers', child: Text(context.l10n.composeVisibilityFollowers)),
                                    PopupMenuItem(value: 'direct', child: Text(context.l10n.composeVisibilityDirect)),
                                  ],
                                  child: Container(
                                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                                    decoration: BoxDecoration(
                                      color: Theme.of(context).colorScheme.surfaceContainerHigh,
                                      borderRadius: BorderRadius.circular(20),
                                      border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(140)),
                                    ),
                                    child: Row(
                                      mainAxisSize: MainAxisSize.min,
                                      children: [
                                        Icon(
                                          _visibility == 'public'
                                              ? Icons.public
                                              : _visibility == 'home'
                                                  ? Icons.home_outlined
                                                  : _visibility == 'direct'
                                                      ? Icons.mail_outline
                                                      : Icons.lock_outline,
                                          size: 16,
                                        ),
                                        const SizedBox(width: 6),
                                        Text(_visibilityLabel(context), style: const TextStyle(fontWeight: FontWeight.w700)),
                                      ],
                                    ),
                                  ),
                                ),
                                const Spacer(),
                                FilledButton(
                                  onPressed: widget.appState.isRunning && !_posting ? _post : null,
                                  child: Text(context.l10n.composePost),
                                ),
                              ],
                            ),
                            const SizedBox(height: 6),
                            Align(
                              alignment: Alignment.centerRight,
                              child: Text(
                                context.l10n.composeCharCount(used, maxChars),
                                style: TextStyle(
                                  fontSize: 12,
                                  color: used > maxChars ? Theme.of(context).colorScheme.error : Theme.of(context).colorScheme.onSurface.withAlpha(160),
                                ),
                              ),
                            ),
                            const SizedBox(height: 6),
                            Container(
                              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                              decoration: BoxDecoration(
                                color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120),
                                borderRadius: BorderRadius.circular(16),
                                border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(120)),
                              ),
                              child: TextField(
                                controller: _text,
                                maxLines: 16,
                                minLines: 6,
                                maxLength: maxChars,
                                onChanged: _onTextChanged,
                                decoration: InputDecoration(
                                  hintText: context.l10n.composeWhatsHappening,
                                  border: InputBorder.none,
                                  counterText: '',
                                ),
                              ),
                            ),
                            if (_mentionSuggestions.isNotEmpty)
                              _MentionSuggestions(
                                suggestions: _mentionSuggestions,
                                searching: _mentionSearching,
                                onPick: _applyMention,
                              ),
                            if (_showPreview && _text.text.trim().isNotEmpty) ...[
                              const SizedBox(height: 10),
                              Container(
                                padding: const EdgeInsets.all(12),
                                decoration: BoxDecoration(
                                  color: Theme.of(context).colorScheme.surfaceContainerHigh.withAlpha(140),
                                  borderRadius: BorderRadius.circular(14),
                                  border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(100)),
                                ),
                                child: HtmlWidget(
                                  MfmCodec.toHtml(_text.text.trim()),
                                ),
                              ),
                            ],
                            const SizedBox(height: 10),
                            Container(
                              padding: const EdgeInsets.all(10),
                              decoration: BoxDecoration(
                                color: Theme.of(context).colorScheme.surfaceContainerHigh.withAlpha(140),
                                borderRadius: BorderRadius.circular(12),
                                border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(100)),
                              ),
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  Wrap(
                                    spacing: 6,
                                    runSpacing: 6,
                                    crossAxisAlignment: WrapCrossAlignment.center,
                                    children: [
                                      IconButton(
                                        tooltip: context.l10n.composeAddMedia,
                                        onPressed: widget.appState.isRunning && !_posting ? _pickMedia : null,
                                        icon: const Icon(Icons.attach_file),
                                      ),
                                      IconButton(
                                        tooltip: context.l10n.composeEmojiButton,
                                        onPressed: () async {
                                          final picked = await EmojiPicker.show(context, prefs: widget.appState.prefs);
                                          if (picked == null) return;
                                          _insertEmoji(_text, picked);
                                          await EmojiRecentStore.add(picked);
                                        },
                                        icon: const Icon(Icons.emoji_emotions_outlined),
                                      ),
                                      IconButton(
                                        tooltip: 'Bold',
                                        onPressed: () => _wrapSelection(_text, '**', '**'),
                                        icon: const Icon(Icons.format_bold),
                                      ),
                                      IconButton(
                                        tooltip: 'Italic',
                                        onPressed: () => _wrapSelection(_text, '*', '*'),
                                        icon: const Icon(Icons.format_italic),
                                      ),
                                      IconButton(
                                        tooltip: 'Strike',
                                        onPressed: () => _wrapSelection(_text, '~~', '~~'),
                                        icon: const Icon(Icons.strikethrough_s),
                                      ),
                                      IconButton(
                                        tooltip: 'Code',
                                        onPressed: () => _wrapSelection(_text, '`', '`'),
                                        icon: const Icon(Icons.code),
                                      ),
                                      IconButton(
                                        tooltip: 'Quote',
                                        onPressed: () => _prefixSelectionLines(_text, '> '),
                                        icon: const Icon(Icons.format_quote),
                                      ),
                                      IconButton(
                                        tooltip: context.l10n.composeMfmCheatsheet,
                                        onPressed: () => MfmCheatsheet.show(context),
                                        icon: const Icon(Icons.text_snippet_outlined),
                                      ),
                                    ],
                                  ),
                                  const SizedBox(height: 8),
                                  Row(
                                    children: [
                                      Tooltip(
                                        message: context.l10n.composeContentWarningHint,
                                        child: IconButton(
                                          onPressed: () => setState(() => _cwEnabled = !_cwEnabled),
                                          icon: Icon(_cwEnabled ? Icons.warning_amber : Icons.warning_amber_outlined),
                                          style: IconButton.styleFrom(
                                            foregroundColor: _cwEnabled ? Theme.of(context).colorScheme.primary : null,
                                          ),
                                        ),
                                      ),
                                      Tooltip(
                                        message: context.l10n.composeSensitiveMediaHint,
                                        child: IconButton(
                                          onPressed: () => setState(() => _sensitive = !_sensitive),
                                          icon: Icon(_sensitive ? Icons.visibility_off : Icons.visibility_off_outlined),
                                          style: IconButton.styleFrom(
                                            foregroundColor: _sensitive ? Theme.of(context).colorScheme.primary : null,
                                          ),
                                        ),
                                      ),
                                      IconButton(
                                        tooltip: _showPreview ? 'Hide preview' : 'Preview',
                                        onPressed: () => setState(() => _showPreview = !_showPreview),
                                        icon: Icon(_showPreview ? Icons.visibility : Icons.visibility_outlined),
                                      ),
                                      const Spacer(),
                                      IconButton(
                                        tooltip: context.l10n.composeExpand,
                                        onPressed: _openExpandedComposer,
                                        icon: const Icon(Icons.open_in_full),
                                      ),
                                    ],
                                  ),
                                ],
                              ),
                            ),
                            if (_cwEnabled) ...[
                              const SizedBox(height: 8),
                              TextField(
                                controller: _cw,
                                decoration: InputDecoration(labelText: context.l10n.composeContentWarningTextLabel),
                              ),
                            ],
                            if (_visibility == 'direct') ...[
                              const SizedBox(height: 8),
                              _DirectRecipientField(
                                controller: _directTo,
                                recent: _directRecent,
                                hint: context.l10n.composeVisibilityDirectHint,
                              ),
                            ],
                            if (_media.isNotEmpty) ...[
                              const SizedBox(height: 8),
                              Text(
                                '${context.l10n.composeAttachments} (${_media.length})',
                                style: const TextStyle(fontWeight: FontWeight.w700),
                              ),
                              const SizedBox(height: 6),
                              for (var i = 0; i < _media.length; i++)
                                Card(
                                  child: ListTile(
                                    dense: true,
                                    title: Text(_media[i].name, maxLines: 1, overflow: TextOverflow.ellipsis),
                                    subtitle: Text(
                                      _media[i].coreMediaId == null ? context.l10n.composeNotUploaded : context.l10n.composeMediaId(_media[i].coreMediaId!),
                                    ),
                                    trailing: Row(
                                      mainAxisSize: MainAxisSize.min,
                                      children: [
                                        IconButton(
                                          tooltip: 'Move up',
                                          onPressed: _posting || i == 0 ? null : () => setState(() => _swapMedia(i, i - 1)),
                                          icon: const Icon(Icons.keyboard_arrow_up),
                                        ),
                                        IconButton(
                                          tooltip: 'Move down',
                                          onPressed: _posting || i == _media.length - 1 ? null : () => setState(() => _swapMedia(i, i + 1)),
                                          icon: const Icon(Icons.keyboard_arrow_down),
                                        ),
                                        IconButton(
                                          icon: const Icon(Icons.close),
                                          onPressed: _posting ? null : () => setState(() => _media.removeAt(i)),
                                        ),
                                      ],
                                    ),
                                  ),
                                ),
                            ],
                            if (_status != null) ...[
                              const SizedBox(height: 6),
                              Text(
                                _status!,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(color: _status!.startsWith('OK') ? null : Theme.of(context).colorScheme.error),
                              ),
                            ],
                          ],
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
            if (_dragging)
              Positioned.fill(
                child: IgnorePointer(
                  child: Container(
                    decoration: BoxDecoration(
                      color: Colors.black.withAlpha(60),
                      borderRadius: BorderRadius.circular(14),
                      border: Border.all(color: Theme.of(context).colorScheme.primary.withAlpha(200), width: 2),
                    ),
                    alignment: Alignment.center,
                    child: Text(
                      context.l10n.composeDropHere,
                      style: const TextStyle(fontWeight: FontWeight.w800, fontSize: 16),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }

  static const _allowed = <String, List<String>>{
    'image': ['png', 'jpg', 'jpeg', 'gif', 'webp', 'avif', 'heic', 'heif', 'bmp', 'tif', 'tiff'],
    'video': ['mp4', 'webm', 'mkv', 'mov', 'avi', 'm4v'],
    'audio': ['mp3', 'ogg', 'wav', 'flac', 'm4a', 'aac', 'opus'],
  };

  Future<void> _pickMedia() async {
    final groups = <XTypeGroup>[
      XTypeGroup(label: 'Images', extensions: _allowed['image']),
      XTypeGroup(label: 'Video', extensions: _allowed['video']),
      XTypeGroup(label: 'Audio', extensions: _allowed['audio']),
    ];
    try {
      final files = await openFiles(acceptedTypeGroups: groups);
      await _addMediaFiles(files);
    } catch (e) {
      setState(() => _status = context.l10n.composeErrUnablePickFile(e.toString()));
    }
  }

  Future<void> _addMediaFiles(List<XFile> files) async {
    if (files.isEmpty) return;
    for (final f in files) {
      try {
        final bytes = await f.readAsBytes();
        final mime = lookupMimeType(f.name, headerBytes: bytes) ?? '';
        if (!(mime.startsWith('image/') || mime.startsWith('video/') || mime.startsWith('audio/'))) {
          setState(() => _status = context.l10n.composeErrInvalidMediaType(mime.isEmpty ? f.name : mime));
          continue;
        }
        if (!mounted) return;
        setState(() {
          _media.add(_PickedMedia(name: f.name.isEmpty ? context.l10n.composeFileFallback : f.name, bytes: bytes));
        });
      } catch (e) {
        setState(() => _status = context.l10n.composeErrUnableReadFile(e.toString()));
      }
    }
  }

  Future<void> _post() async {
    if (!widget.appState.isRunning) return;
    final text = _text.text.trim();
    if (text.isEmpty) {
      setState(() => _status = context.l10n.composeErrEmptyContent);
      return;
    }
    setState(() {
      _posting = true;
      _status = null;
    });
    try {
      final mediaIds = <String>[];
      for (final m in _media) {
        if (m.coreMediaId != null) {
          mediaIds.add(m.coreMediaId!);
          continue;
        }
        final resp = await widget.api.uploadMedia(bytes: m.bytes, filename: m.name);
        final id = (resp['id'] as String?)?.trim();
        if (id != null && id.isNotEmpty) {
          m.coreMediaId = id;
          mediaIds.add(id);
        }
      }
      if (_visibility == 'direct' && _directTo.text.trim().isEmpty) {
        setState(() => _status = context.l10n.composeVisibilityDirectMissing);
        return;
      }
      final content = MfmCodec.hasMarkers(text) ? MfmCodec.toHtml(text) : text;
      await widget.api.postNote(
        content: content,
        public: _visibility == 'public',
        mediaIds: mediaIds,
        summary: _cwEnabled ? _cw.text : null,
        sensitive: _sensitive,
        visibility: _visibility,
        directTo: _directTo.text.trim().isEmpty ? null : _directTo.text.trim(),
      );
      if (_visibility == 'direct') {
        await DirectRecipientStore.add(_directTo.text.trim());
        final recent = await DirectRecipientStore.read();
        if (mounted) setState(() => _directRecent = recent);
      }
      setState(() {
        _status = context.l10n.composeQueuedOk;
        _text.clear();
        _cw.clear();
        _directTo.clear();
        _media.clear();
      });
      widget.onPosted();
    } catch (e) {
      setState(() => _status = context.l10n.composeErrGeneric(e.toString()));
    } finally {
      setState(() => _posting = false);
    }
  }

  void _swapMedia(int a, int b) {
    if (a < 0 || b < 0 || a >= _media.length || b >= _media.length) return;
    final tmp = _media[a];
    _media[a] = _media[b];
    _media[b] = tmp;
  }

  void _onTextChanged(String _) {
    setState(() {});
    _runMentionSearch();
  }

  void _runMentionSearch() {
    final value = _text.value;
    final cursor = value.selection.baseOffset;
    if (cursor <= 0) {
      _clearMentionSuggestions();
      return;
    }
    final before = value.text.substring(0, cursor);
    final lastSpace = before.lastIndexOf(RegExp(r'[\s\n]'));
    final start = lastSpace == -1 ? 0 : lastSpace + 1;
    final token = before.substring(start);
    if (!token.startsWith('@') || token.length < 2) {
      _clearMentionSuggestions();
      return;
    }
    final query = token.substring(1).trim();
    if (query.isEmpty) {
      _clearMentionSuggestions();
      return;
    }
    if (query == _mentionQuery) return;
    _mentionQuery = query;
    _mentionStart = start;
    _mentionDebounce?.cancel();
    _mentionDebounce = Timer(const Duration(milliseconds: 250), () async {
      if (mounted) setState(() => _mentionSearching = true);
      try {
        final resp = await widget.api.searchUsers(
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
        if (mounted) setState(() => _mentionSuggestions = list);
      } catch (_) {
        if (mounted) setState(() => _mentionSuggestions = const []);
      } finally {
        if (mounted) setState(() => _mentionSearching = false);
      }
    });
  }

  void _applyMention(ActorProfile profile) {
    final value = _text.value;
    final cursor = value.selection.baseOffset;
    if (cursor < 0) return;
    final start = _mentionStart ?? (cursor - _mentionQuery.length - 1);
    if (start < 0 || start > value.text.length) return;
    final handle = _formatHandle(profile);
    final next = value.text.replaceRange(start, cursor, '$handle ');
    final offset = start + handle.length + 1;
    _text.value = value.copyWith(
      text: next,
      selection: TextSelection.collapsed(offset: offset),
      composing: TextRange.empty,
    );
    _clearMentionSuggestions();
  }

  String _formatHandle(ActorProfile profile) {
    final username = profile.preferredUsername.trim();
    final uri = Uri.tryParse(profile.id);
    final host = uri?.host ?? '';
    if (username.isEmpty || host.isEmpty) return '@${profile.displayName}';
    return '@$username@$host';
  }

  void _clearMentionSuggestions() {
    if (_mentionSuggestions.isNotEmpty || _mentionSearching) {
      setState(() {
        _mentionSuggestions = const [];
        _mentionSearching = false;
        _mentionStart = null;
        _mentionQuery = '';
      });
    } else {
      _mentionStart = null;
      _mentionQuery = '';
    }
  }

  void _insertEmoji(TextEditingController ctrl, String emoji) {
    final value = ctrl.value;
    final selection = value.selection;
    final text = value.text;
    final start = selection.start >= 0 ? selection.start : text.length;
    final end = selection.end >= 0 ? selection.end : text.length;
    final next = text.replaceRange(start, end, emoji);
    ctrl.value = TextEditingValue(
      text: next,
      selection: TextSelection.collapsed(offset: start + emoji.length),
    );
  }

  void _wrapSelection(TextEditingController ctrl, String before, String after) {
    final value = ctrl.value;
    final text = value.text;
    var start = value.selection.start >= 0 ? value.selection.start : text.length;
    var end = value.selection.end >= 0 ? value.selection.end : text.length;
    if (start > end) {
      final tmp = start;
      start = end;
      end = tmp;
    }
    final selected = text.substring(start, end);
    final next = text.replaceRange(start, end, '$before$selected$after');
    ctrl.value = TextEditingValue(
      text: next,
      selection: TextSelection.collapsed(offset: start + before.length + selected.length + after.length),
    );
    setState(() {});
  }

  void _prefixSelectionLines(TextEditingController ctrl, String prefix) {
    final value = ctrl.value;
    final text = value.text;
    var start = value.selection.start >= 0 ? value.selection.start : text.length;
    var end = value.selection.end >= 0 ? value.selection.end : text.length;
    if (start > end) {
      final tmp = start;
      start = end;
      end = tmp;
    }
    final lineStart = text.lastIndexOf('\n', start - 1);
    final lineEnd = text.indexOf('\n', end);
    final startIndex = lineStart < 0 ? 0 : lineStart + 1;
    final endIndex = lineEnd < 0 ? text.length : lineEnd;
    final segment = text.substring(startIndex, endIndex);
    final lines = segment.split('\n').map((line) {
      if (line.startsWith(prefix)) return line;
      return '$prefix$line';
    }).join('\n');
    final next = text.replaceRange(startIndex, endIndex, lines);
    ctrl.value = TextEditingValue(
      text: next,
      selection: TextSelection.collapsed(offset: startIndex + lines.length),
    );
    setState(() {});
  }

  String _visibilityLabel(BuildContext context) {
    return switch (_visibility) {
      'home' => context.l10n.composeVisibilityHome,
      'followers' => context.l10n.composeVisibilityFollowers,
      'direct' => context.l10n.composeVisibilityDirect,
      _ => context.l10n.composeVisibilityPublic,
    };
  }

  Future<void> _openExpandedComposer() async {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (context) {
        final inset = MediaQuery.of(context).viewInsets.bottom;
        return SafeArea(
          child: Padding(
            padding: EdgeInsets.only(left: 16, right: 16, top: 12, bottom: inset + 16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    Expanded(child: Text(context.l10n.composeExpandTitle, style: const TextStyle(fontWeight: FontWeight.w800))),
                    IconButton(onPressed: () => Navigator.of(context).pop(), icon: const Icon(Icons.close)),
                  ],
                ),
                const SizedBox(height: 8),
                TextField(
                  controller: _text,
                  minLines: 8,
                  maxLines: 18,
                  decoration: InputDecoration(hintText: context.l10n.composeWhatsHappening),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _MentionSuggestions extends StatelessWidget {
  const _MentionSuggestions({
    required this.suggestions,
    required this.searching,
    required this.onPick,
  });

  final List<ActorProfile> suggestions;
  final bool searching;
  final ValueChanged<ActorProfile> onPick;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Container(
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surfaceContainerHigh.withAlpha(180),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(100)),
        ),
        child: Column(
          children: [
            if (searching)
              const Padding(
                padding: EdgeInsets.all(8),
                child: LinearProgressIndicator(minHeight: 2),
              ),
            for (final profile in suggestions)
              InkWell(
                onTap: () => onPick(profile),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                  child: Row(
                    children: [
                      StatusAvatar(
                        imageUrl: profile.iconUrl,
                        size: 24,
                        showStatus: profile.isFedi3,
                        statusKey: profile.statusKey,
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              profile.displayName,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(fontWeight: FontWeight.w700),
                            ),
                            Text(
                              _formatHandle(profile),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 12,
                                color: Theme.of(context).colorScheme.onSurface.withAlpha(160),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }

  static String _formatHandle(ActorProfile profile) {
    final username = profile.preferredUsername.trim();
    final uri = Uri.tryParse(profile.id);
    final host = uri?.host ?? '';
    if (username.isEmpty || host.isEmpty) return '@${profile.displayName}';
    return '@$username@$host';
  }
}

class _DirectRecipientField extends StatelessWidget {
  const _DirectRecipientField({
    required this.controller,
    required this.recent,
    required this.hint,
  });

  final TextEditingController controller;
  final List<String> recent;
  final String hint;

  @override
  Widget build(BuildContext context) {
    return Autocomplete<String>(
      optionsBuilder: (value) {
        final q = value.text.trim().toLowerCase();
        if (q.isEmpty) return recent;
        return recent.where((r) => r.toLowerCase().contains(q));
      },
      onSelected: (v) => controller.text = v,
      fieldViewBuilder: (context, textCtrl, focusNode, onFieldSubmitted) {
        textCtrl.value = controller.value;
        return TextField(
          controller: textCtrl,
          focusNode: focusNode,
          decoration: InputDecoration(hintText: hint),
          onChanged: (_) => controller.value = textCtrl.value,
          onSubmitted: (_) => onFieldSubmitted(),
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
}

class _TinyAvatar extends StatelessWidget {
  const _TinyAvatar({
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
