/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../../state/draft_store.dart';
import '../widgets/emoji_picker.dart';
import '../../state/emoji_recent_store.dart';
import '../../state/direct_recipient_store.dart';
import '../widgets/mfm_cheatsheet.dart';
import '../../utils/mfm_codec.dart';
import '../widgets/status_avatar.dart';

class ComposeScreen extends StatefulWidget {
  const ComposeScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<ComposeScreen> createState() => _ComposeScreenState();
}

class _ComposeScreenState extends State<ComposeScreen> {
  static const maxChars = 7000;
  final _text = TextEditingController();
  final _cw = TextEditingController();
  final _filePath = TextEditingController();
  String _visibility = 'public';
  final _directTo = TextEditingController();
  List<String> _directRecent = const [];
  bool _cwEnabled = false;
  bool _sensitive = false;
  bool _posting = false;
  String? _status;
  String? _draftStatus;
  final _media = <_PickedMedia>[];
  bool _restoredDraft = false;
  Timer? _draftDebounce;
  bool _showPreview = false;
  Timer? _mentionDebounce;
  bool _mentionSearching = false;
  List<ActorProfile> _mentionSuggestions = const [];
  int? _mentionStart;
  String _mentionQuery = '';

  @override
  void initState() {
    super.initState();
    _loadDraft();
    _loadDirectRecent();
    _text.addListener(_scheduleDraftSave);
  }

  @override
  void dispose() {
    _draftDebounce?.cancel();
    _mentionDebounce?.cancel();
    _text.removeListener(_scheduleDraftSave);
    _text.dispose();
    _cw.dispose();
    _directTo.dispose();
    _filePath.dispose();
    super.dispose();
  }

  Future<void> _loadDirectRecent() async {
    final recent = await DirectRecipientStore.read();
    if (!mounted) return;
    setState(() => _directRecent = recent);
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config!;
    final api = CoreApi(config: cfg);
    final used = _text.text.characters.length;

    return Shortcuts(
      shortcuts: {
        // Windows/Linux.
        LogicalKeySet(LogicalKeyboardKey.control, LogicalKeyboardKey.enter): const _SubmitComposeIntent(),
        LogicalKeySet(LogicalKeyboardKey.control, LogicalKeyboardKey.numpadEnter): const _SubmitComposeIntent(),
        // macOS.
        LogicalKeySet(LogicalKeyboardKey.meta, LogicalKeyboardKey.enter): const _SubmitComposeIntent(),
        LogicalKeySet(LogicalKeyboardKey.meta, LogicalKeyboardKey.numpadEnter): const _SubmitComposeIntent(),
      },
      child: Actions(
        actions: {
          _SubmitComposeIntent: CallbackAction<_SubmitComposeIntent>(
            onInvoke: (_) {
              if (!widget.appState.isRunning || _posting) return null;
              _post(api);
              return null;
            },
          ),
        },
        child: Scaffold(
          appBar: AppBar(title: Text(context.l10n.composeTitle)),
          body: ListView(
            padding: const EdgeInsets.all(16),
            children: [
          if (!widget.appState.isRunning)
            Card(
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Text(
                  context.l10n.composeCoreNotRunning(context.l10n.navSettings, context.l10n.devCoreStart),
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            ),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      PopupMenuButton<String>(
                        tooltip: context.l10n.composeVisibilityTitle,
                        onSelected: (v) {
                          setState(() => _visibility = v);
                          _scheduleDraftSave();
                        },
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
                      FilledButton.icon(
                        onPressed: widget.appState.isRunning && !_posting ? () => _post(api) : null,
                        icon: const Icon(Icons.send),
                        label: Text(context.l10n.composePost),
                      ),
                    ],
                  ),
                  if (_draftStatus != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      _draftStatus!,
                      style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160), fontSize: 12),
                    ),
                  ],
                  const SizedBox(height: 8),
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
                      minLines: 6,
                      maxLines: 12,
                      maxLength: maxChars,
                      decoration: InputDecoration(
                        hintText: context.l10n.composeWhatsHappening,
                        border: InputBorder.none,
                        counterText: '',
                      ),
                      onChanged: _onTextChanged,
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
                      child: HtmlWidget(MfmCodec.toHtml(_text.text.trim())),
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
                          children: [
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
                            FilterChip(
                              label: Text(context.l10n.composeContentWarningTitle),
                              selected: _cwEnabled,
                              onSelected: (v) {
                                setState(() => _cwEnabled = v);
                                _scheduleDraftSave();
                              },
                            ),
                            const SizedBox(width: 6),
                            FilterChip(
                              label: Text(context.l10n.composeSensitiveMediaTitle),
                              selected: _sensitive,
                              onSelected: (v) {
                                setState(() => _sensitive = v);
                                _scheduleDraftSave();
                              },
                            ),
                            const Spacer(),
                            IconButton(
                              tooltip: _showPreview ? 'Hide preview' : 'Preview',
                              onPressed: () => setState(() => _showPreview = !_showPreview),
                              icon: Icon(_showPreview ? Icons.visibility : Icons.visibility_outlined),
                            ),
                            OutlinedButton.icon(
                              onPressed: _posting ? null : _clearDraft,
                              icon: const Icon(Icons.delete_outline),
                              label: Text(context.l10n.composeClearDraft),
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
          if (_cwEnabled) ...[
            const SizedBox(height: 10),
            TextField(
              controller: _cw,
              decoration: InputDecoration(labelText: context.l10n.composeContentWarningTextLabel),
              onChanged: (_) => _scheduleDraftSave(),
            ),
          ],
          if (_visibility == 'direct') ...[
            const SizedBox(height: 10),
            _DirectRecipientField(
              controller: _directTo,
              recent: _directRecent,
              label: context.l10n.composeVisibilityDirectLabel,
              hint: context.l10n.composeVisibilityDirectHint,
              onChanged: _scheduleDraftSave,
            ),
          ],
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _filePath,
                  decoration: InputDecoration(
                    labelText: context.l10n.composeMediaFilePathLabel,
                    hintText: context.l10n.composeMediaFilePathHint,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              FilledButton.icon(
                onPressed: widget.appState.isRunning ? _addMediaFromPath : null,
                icon: const Icon(Icons.attach_file),
                label: Text(context.l10n.composeAddMediaPath),
              ),
            ],
          ),
          const SizedBox(height: 12),
          if (_status != null)
            Text(
              _status!,
              style: TextStyle(color: _status!.startsWith('OK') ? null : Theme.of(context).colorScheme.error),
            ),
          if (_media.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text(context.l10n.composeAttachments, style: const TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 8),
            for (var i = 0; i < _media.length; i++)
              Card(
                child: ListTile(
                  title: Text(_media[i].name),
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
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _loadDraft() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final draft = await DraftStore.read(username: cfg.username, domain: cfg.domain);
    if (!mounted || draft == null) return;
    final hasText = draft.text.trim().isNotEmpty;
    if (!hasText) return;

    setState(() {
      _text.text = draft.text;
      _visibility = draft.visibility;
      _directTo.text = draft.directTo;
      _cw.text = draft.summary;
      _cwEnabled = draft.summary.trim().isNotEmpty;
      _sensitive = draft.sensitive;
      _restoredDraft = true;
      _draftStatus = context.l10n.composeDraftRestored;
    });
  }

  void _scheduleDraftSave() {
    if (!mounted) return;
    if (_posting) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;

    final txt = _text.text;
    final visibility = _visibility;
    final directTo = _directTo.text.trim();
    final summary = _cwEnabled ? _cw.text : '';
    final sensitive = _sensitive;
    final shouldSave = txt.trim().isNotEmpty || _restoredDraft;
    if (!shouldSave) return;

    _draftDebounce?.cancel();
    _draftDebounce = Timer(const Duration(milliseconds: 450), () async {
      final now = DateTime.now();
      final draft = ComposeDraft(
        text: txt,
        isPublic: visibility == 'public',
        summary: summary,
        sensitive: sensitive,
        visibility: visibility,
        directTo: directTo,
        updatedAtMs: now.millisecondsSinceEpoch,
      );
      await DraftStore.write(username: cfg.username, domain: cfg.domain, draft: draft);
      if (!mounted) return;
      setState(() {
        _draftStatus = context.l10n.composeDraftSaved;
      });
    });
  }

  Future<void> _clearDraft() async {
    if (_posting) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    await DraftStore.clear(username: cfg.username, domain: cfg.domain);
    setState(() {
      _text.clear();
      _cw.clear();
      _directTo.clear();
      _media.clear();
      _status = null;
      _draftStatus = context.l10n.composeDraftCleared;
      _restoredDraft = false;
    });
  }

  Future<void> _addMediaFromPath() async {
    final path = _filePath.text.trim();
    if (path.isEmpty) return;
    try {
      final file = File(path);
      final bytes = await file.readAsBytes();
      final name = path.split(RegExp(r'[\\/]+')).last;
      setState(() {
        _media.add(_PickedMedia(name: name.isEmpty ? context.l10n.composeFileFallback : name, bytes: bytes));
        _filePath.clear();
      });
    } catch (e) {
      setState(() => _status = context.l10n.composeErrUnableReadFile(e.toString()));
    }
  }

  Future<void> _post(CoreApi api) async {
    final text = _text.text.trim();
    if (text.isEmpty) {
      setState(() => _status = context.l10n.composeErrEmptyContent);
      return;
    }
    if (_visibility == 'direct' && _directTo.text.trim().isEmpty) {
      setState(() => _status = context.l10n.composeVisibilityDirectMissing);
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
        final resp = await api.uploadMedia(bytes: m.bytes, filename: m.name);
        final id = (resp['id'] as String?)?.trim();
        if (id != null && id.isNotEmpty) {
          m.coreMediaId = id;
          mediaIds.add(id);
        }
      }
      final content = MfmCodec.hasMarkers(text) ? MfmCodec.toHtml(text) : text;
      await api.postNote(
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
      final cfg = widget.appState.config;
      if (cfg != null) {
        await DraftStore.clear(username: cfg.username, domain: cfg.domain);
      }
      setState(() {
        _status = context.l10n.composeQueuedOk;
        _text.clear();
        _cw.clear();
        _directTo.clear();
        _media.clear();
        _draftStatus = null;
        _restoredDraft = false;
      });
      if (mounted) Navigator.of(context).maybePop();
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
    _scheduleDraftSave();
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
        final cfg = widget.appState.config;
        if (cfg == null) return;
        final resp = await CoreApi(config: cfg).searchUsers(
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
    _scheduleDraftSave();
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
    _scheduleDraftSave();
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
    _scheduleDraftSave();
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

}

class _DirectRecipientField extends StatelessWidget {
  const _DirectRecipientField({
    required this.controller,
    required this.recent,
    required this.label,
    required this.hint,
    required this.onChanged,
  });

  final TextEditingController controller;
  final List<String> recent;
  final String label;
  final String hint;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    return Autocomplete<String>(
      optionsBuilder: (value) {
        final q = value.text.trim().toLowerCase();
        if (q.isEmpty) return recent;
        return recent.where((r) => r.toLowerCase().contains(q));
      },
      onSelected: (v) {
        controller.text = v;
        onChanged();
      },
      fieldViewBuilder: (context, textCtrl, focusNode, onFieldSubmitted) {
        textCtrl.value = controller.value;
        return TextField(
          controller: textCtrl,
          focusNode: focusNode,
          decoration: InputDecoration(labelText: label, hintText: hint),
          onChanged: (_) {
            controller.value = textCtrl.value;
            onChanged();
          },
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

class _SubmitComposeIntent extends Intent {
  const _SubmitComposeIntent();
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
