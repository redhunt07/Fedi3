/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';
import '../../state/draft_store.dart';
import '../widgets/emoji_picker.dart';
import '../widgets/rich_composer.dart';
import '../../state/emoji_recent_store.dart';
import '../../state/direct_recipient_store.dart';
import '../widgets/mfm_cheatsheet.dart';
import '../../utils/mfm_codec.dart';

class ComposeScreen extends StatefulWidget {
  const ComposeScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<ComposeScreen> createState() => _ComposeScreenState();
}

class _ComposeScreenState extends State<ComposeScreen> {
  final _text = TextEditingController();
  final _cw = TextEditingController();
  String _visibility = 'public';
  final _directTo = TextEditingController();
  List<String> _directRecent = const [];
  bool _cwEnabled = false;
  bool _sensitive = false;
  bool _posting = false;
  bool _dragging = false;
  String? _status;
  String? _draftStatus;
  final _media = <_PickedMedia>[];
  bool _restoredDraft = false;
  bool _hasText = false;
  Timer? _draftDebounce;

  @override
  void initState() {
    super.initState();
    _loadDraft();
    _loadDirectRecent();
    _text.addListener(_onTextChanged);
    _text.addListener(_scheduleDraftSave);
  }

  @override
  void dispose() {
    _draftDebounce?.cancel();
    _text.removeListener(_onTextChanged);
    _text.removeListener(_scheduleDraftSave);
    _text.dispose();
    _cw.dispose();
    _directTo.dispose();
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

    return Shortcuts(
      shortcuts: {
        // Windows/Linux.
        LogicalKeySet(LogicalKeyboardKey.control, LogicalKeyboardKey.enter):
            const _SubmitComposeIntent(),
        LogicalKeySet(
                LogicalKeyboardKey.control, LogicalKeyboardKey.numpadEnter):
            const _SubmitComposeIntent(),
        // macOS.
        LogicalKeySet(LogicalKeyboardKey.meta, LogicalKeyboardKey.enter):
            const _SubmitComposeIntent(),
        LogicalKeySet(LogicalKeyboardKey.meta, LogicalKeyboardKey.numpadEnter):
            const _SubmitComposeIntent(),
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
          body: DropTarget(
            onDragEntered: (_) => setState(() => _dragging = true),
            onDragExited: (_) => setState(() => _dragging = false),
            onDragDone: (detail) async {
              setState(() => _dragging = false);
              await _addFiles(detail.files);
            },
            child: Stack(
              children: [
                ListView(
                  padding: const EdgeInsets.all(16),
                  children: [
                    if (!widget.appState.isRunning)
                      Card(
                        child: Padding(
                          padding: const EdgeInsets.all(12),
                          child: Text(
                            context.l10n.composeCoreNotRunning(
                                context.l10n.navSettings,
                                context.l10n.devCoreStart),
                            style: TextStyle(
                                color: Theme.of(context).colorScheme.error),
                          ),
                        ),
                      ),
                    RichComposer(
                      controller: _text,
                      hintText: context.l10n.composeWhatsHappening,
                      enabled: widget.appState.isRunning,
                      onEmoji: () => EmojiPicker.show(context,
                          prefs: widget.appState.prefs),
                      onMedia: widget.appState.isRunning ? _pickFiles : null,
                    ),
                    if (_draftStatus != null) ...[
                      const SizedBox(height: 8),
                      Text(
                        _draftStatus!,
                        style: TextStyle(
                            color: Theme.of(context)
                                .colorScheme
                                .onSurface
                                .withAlpha(160),
                            fontSize: 12),
                      ),
                    ],
                    const SizedBox(height: 12),
                    Wrap(
                      spacing: 10,
                      runSpacing: 10,
                      children: [
                        OutlinedButton.icon(
                          onPressed:
                              widget.appState.isRunning ? _pickFiles : null,
                          icon: const Icon(Icons.attach_file),
                          label: Text(context.l10n.composeChooseFiles),
                        ),
                        OutlinedButton.icon(
                          onPressed: () async {
                            final picked = await EmojiPicker.show(context,
                                prefs: widget.appState.prefs);
                            if (picked == null) return;
                            _insertEmoji(_text, picked);
                            await EmojiRecentStore.add(picked);
                          },
                          icon: const Icon(Icons.emoji_emotions_outlined),
                          label: Text(context.l10n.composeEmojiButton),
                        ),
                        OutlinedButton.icon(
                          onPressed: () => MfmCheatsheet.show(context),
                          icon: const Icon(Icons.text_snippet_outlined),
                          label: Text(context.l10n.composeMfmCheatsheet),
                        ),
                        FilterChip(
                          label: Text(context.l10n.composeContentWarningTitle),
                          selected: _cwEnabled,
                          onSelected: (v) {
                            setState(() => _cwEnabled = v);
                            _scheduleDraftSave();
                          },
                        ),
                        FilterChip(
                          label: Text(context.l10n.composeSensitiveMediaTitle),
                          selected: _sensitive,
                          onSelected: (v) {
                            setState(() => _sensitive = v);
                            _scheduleDraftSave();
                          },
                        ),
                        PopupMenuButton<String>(
                          tooltip: context.l10n.composeVisibilityTitle,
                          onSelected: (v) {
                            setState(() => _visibility = v);
                            _scheduleDraftSave();
                          },
                          itemBuilder: (context) => [
                            PopupMenuItem(
                                value: 'public',
                                child:
                                    Text(context.l10n.composeVisibilityPublic)),
                            PopupMenuItem(
                                value: 'home',
                                child:
                                    Text(context.l10n.composeVisibilityHome)),
                            PopupMenuItem(
                                value: 'followers',
                                child: Text(
                                    context.l10n.composeVisibilityFollowers)),
                            PopupMenuItem(
                                value: 'direct',
                                child:
                                    Text(context.l10n.composeVisibilityDirect)),
                          ],
                          icon: Icon(
                            _visibility == 'public'
                                ? Icons.public
                                : _visibility == 'home'
                                    ? Icons.home_outlined
                                    : _visibility == 'direct'
                                        ? Icons.mail_outline
                                        : Icons.lock_outline,
                          ),
                        ),
                        OutlinedButton.icon(
                          onPressed: _posting ? null : _clearDraft,
                          icon: const Icon(Icons.delete_outline),
                          label: Text(context.l10n.composeClearDraft),
                        ),
                        FilledButton.icon(
                          // AW-06: an empty post cannot be submitted.
                          onPressed:
                              widget.appState.isRunning && !_posting && _hasText
                                  ? () => _post(api)
                                  : null,
                          icon: const Icon(Icons.send),
                          label: Text(context.l10n.composePost),
                        ),
                      ],
                    ),
                    if (_cwEnabled) ...[
                      const SizedBox(height: 10),
                      TextField(
                        controller: _cw,
                        decoration: InputDecoration(
                            labelText:
                                context.l10n.composeContentWarningTextLabel),
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
                    Container(
                      padding: const EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: Theme.of(context)
                            .colorScheme
                            .surfaceContainerHighest
                            .withAlpha(120),
                        borderRadius: BorderRadius.circular(16),
                        border: Border.all(
                          color: _dragging
                              ? Theme.of(context).colorScheme.primary
                              : Theme.of(context).colorScheme.outlineVariant,
                          width: _dragging ? 1.6 : 1,
                        ),
                      ),
                      child: Row(
                        children: [
                          Container(
                            width: 42,
                            height: 42,
                            decoration: BoxDecoration(
                              shape: BoxShape.circle,
                              color: Theme.of(context)
                                  .colorScheme
                                  .primary
                                  .withAlpha(22),
                            ),
                            child: Icon(
                              _dragging
                                  ? Icons.file_download_rounded
                                  : Icons.cloud_upload_outlined,
                              color: Theme.of(context).colorScheme.primary,
                            ),
                          ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  context.l10n.composeDropMediaTitle,
                                  style: const TextStyle(
                                      fontWeight: FontWeight.w700,
                                      fontSize: 13),
                                ),
                                const SizedBox(height: 2),
                                Text(
                                  context.l10n.composeDropMediaHint,
                                  style: TextStyle(
                                    fontSize: 11.5,
                                    color: Theme.of(context)
                                        .colorScheme
                                        .onSurfaceVariant,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          const SizedBox(width: 8),
                          OutlinedButton.icon(
                            onPressed: _posting ? null : _pickFiles,
                            icon:
                                const Icon(Icons.attach_file_rounded, size: 16),
                            label: Text(context.l10n.composeChooseFiles),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 12),
                    if (_status != null)
                      Text(
                        _status!,
                        style: TextStyle(
                            color: _status!.startsWith('OK')
                                ? null
                                : Theme.of(context).colorScheme.error),
                      ),
                    if (_media.isNotEmpty) ...[
                      const SizedBox(height: 12),
                      Text(context.l10n.composeAttachments,
                          style: const TextStyle(fontWeight: FontWeight.w700)),
                      const SizedBox(height: 8),
                      for (var i = 0; i < _media.length; i++)
                        Card(
                          child: ListTile(
                            title: Text(_media[i].name),
                            subtitle: Text(
                              _media[i].coreMediaId == null
                                  ? context.l10n.composeNotUploaded
                                  : context.l10n
                                      .composeMediaId(_media[i].coreMediaId!),
                            ),
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                IconButton(
                                  tooltip: 'Move up',
                                  onPressed: _posting || i == 0
                                      ? null
                                      : () =>
                                          setState(() => _swapMedia(i, i - 1)),
                                  icon: const Icon(Icons.keyboard_arrow_up),
                                ),
                                IconButton(
                                  tooltip: 'Move down',
                                  onPressed: _posting || i == _media.length - 1
                                      ? null
                                      : () =>
                                          setState(() => _swapMedia(i, i + 1)),
                                  icon: const Icon(Icons.keyboard_arrow_down),
                                ),
                                IconButton(
                                  icon: const Icon(Icons.close),
                                  onPressed: _posting
                                      ? null
                                      : () =>
                                          setState(() => _media.removeAt(i)),
                                ),
                              ],
                            ),
                          ),
                        ),
                    ],
                  ],
                ),
                if (_dragging)
                  Positioned.fill(
                    child: IgnorePointer(
                      child: Container(
                        margin: const EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: Theme.of(context)
                              .colorScheme
                              .primary
                              .withAlpha(18),
                          borderRadius: BorderRadius.circular(20),
                          border: Border.all(
                            color: Theme.of(context).colorScheme.primary,
                            width: 2,
                          ),
                        ),
                        child: Center(
                          child: Column(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(Icons.file_download_rounded,
                                  size: 40,
                                  color: Theme.of(context).colorScheme.primary),
                              const SizedBox(height: 8),
                              Text(context.l10n.composeDropMediaTitle,
                                  style: const TextStyle(
                                      fontWeight: FontWeight.w800)),
                            ],
                          ),
                        ),
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

  void _onTextChanged() {
    final hasText = _text.text.trim().isNotEmpty;
    if (hasText != _hasText && mounted) {
      setState(() => _hasText = hasText);
    }
  }

  Future<void> _loadDraft() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final draft =
        await DraftStore.read(username: cfg.username, domain: cfg.domain);
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
      await DraftStore.write(
          username: cfg.username, domain: cfg.domain, draft: draft);
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

  Future<void> _pickFiles() async {
    if (_posting) return;
    try {
      final files = await openFiles();
      await _addFiles(files);
    } catch (e) {
      setState(
          () => _status = context.l10n.composeErrUnableReadFile(e.toString()));
    }
  }

  Future<void> _addFiles(List<XFile> files) async {
    if (files.isEmpty) return;
    final room = 8 - _media.length;
    if (room <= 0) return;
    for (final file in files.take(room)) {
      try {
        final bytes = await file.readAsBytes();
        if (!mounted) return;
        setState(() {
          _media.add(_PickedMedia(
              name: file.name.isEmpty
                  ? context.l10n.composeFileFallback
                  : file.name,
              bytes: bytes));
        });
      } catch (e) {
        if (!mounted) return;
        setState(() =>
            _status = context.l10n.composeErrUnableReadFile(e.toString()));
      }
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
      // D4/H-62: the outbox content is text/html, so plain text must be
      // escaped and wrapped in HTML even when no MFM marker is present.
      final content = MfmCodec.toHtml(text);
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
