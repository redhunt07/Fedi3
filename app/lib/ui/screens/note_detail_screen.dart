/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/note_models.dart';
import '../../services/object_repository.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/note_card.dart';

class NoteDetailScreen extends StatefulWidget {
  const NoteDetailScreen({
    super.key,
    required this.appState,
    required this.activity,
  });

  final AppState appState;
  final Map<String, dynamic> activity;

  @override
  State<NoteDetailScreen> createState() => _NoteDetailScreenState();
}

class _NoteDetailScreenState extends State<NoteDetailScreen> {
  final _notes = <String, Note>{};
  final _children = <String, List<String>>{};
  final _parentsById = <String, String>{};
  final _collapsed = <String>{};
  final _nodeKeys = <String, GlobalKey>{};
  final ScrollController _scroll = ScrollController();
  String? _rootId;
  String? _currentId;
  bool _loading = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    unawaited(_loadContext());
  }

  Future<void> _loadContext() async {
    final item = TimelineItem.tryFromActivity(widget.activity);
    final current = item?.note;
    if (current == null) return;
    _currentId = current.id;

    setState(() {
      _loading = true;
      _error = null;
      _notes.clear();
      _children.clear();
      _parentsById.clear();
    });

    try {
      final api = CoreApi(config: widget.appState.config!);
      _addNote(current);
      var cur = current.inReplyTo.trim();
      for (var i = 0; i < 10; i++) {
        if (cur.isEmpty) break;
        Map<String, dynamic>? obj = await api.fetchCachedObject(cur);
        obj ??= await ObjectRepository.instance.fetchObject(cur);
        if (obj == null) break;
        final note = Note.tryParse(obj);
        if (note == null) break;
        _addNote(note);
        cur = note.inReplyTo.trim();
      }

      _rootId = _notes.values
          .where((n) => n.inReplyTo.trim().isEmpty)
          .map((n) => n.id)
          .firstWhere((_) => true, orElse: () => current.id);

      final queue = <String>[_rootId!];
      final seen = <String>{_rootId!};
      const maxNodes = 200;

      while (queue.isNotEmpty && _notes.length < maxNodes) {
        final id = queue.removeAt(0);
        final resp = await api.fetchNoteReplies(id, limit: 50);
        final items = (resp['items'] as List<dynamic>? ?? const [])
            .whereType<Map>()
            .map((m) => m.cast<String, dynamic>())
            .toList();
        for (final a in items) {
          final note = _noteFromActivity(a);
          if (note == null) continue;
          _addNote(note);
          final nid = note.id;
          if (nid.isEmpty || seen.contains(nid)) continue;
          seen.add(nid);
          queue.add(nid);
          if (_notes.length >= maxNodes) break;
        }
      }

      _rebuildChildren();
    } catch (e) {
      _error = e.toString();
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  void _addNote(Note note) {
    final id = note.id.trim();
    if (id.isEmpty) return;
    _notes[id] = note;
    final parent = note.inReplyTo.trim();
    if (parent.isNotEmpty) {
      _parentsById[id] = parent;
    }
  }

  void _rebuildChildren() {
    _children.clear();
    for (final entry in _notes.entries) {
      final parent = _parentsById[entry.key];
      if (parent == null) continue;
      if (!_notes.containsKey(parent)) continue;
      _children.putIfAbsent(parent, () => []).add(entry.key);
    }
  }

  Note? _noteFromActivity(Map<String, dynamic> activity) {
    final item = TimelineItem.tryFromActivity(activity);
    return item?.note;
  }

  @override
  Widget build(BuildContext context) {
    final item = TimelineItem.tryFromActivity(widget.activity);
    final rootId = _rootId ?? item?.note.id ?? '';
    final entries = _flattenThread(rootId);
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.noteThreadTitle)),
      body: ListView(
        controller: _scroll,
        padding: const EdgeInsets.all(12),
        children: [
          if (_error != null)
            NetworkErrorCard(
              message: _error,
              onRetry: _loadContext,
              compact: true,
            ),
          if (_loading)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 12),
              child: Center(child: CircularProgressIndicator()),
            ),
          if (entries.isNotEmpty)
            for (final entry in entries)
              _ThreadNodeRow(
                key: _nodeKeys.putIfAbsent(entry.note.id, () => GlobalKey()),
                appState: widget.appState,
                note: entry.note,
                depth: entry.depth,
                isRoot: entry.note.id == rootId,
                isCurrent: entry.note.id == _currentId,
                hasChildren: _children[entry.note.id]?.isNotEmpty == true,
                collapsed: _collapsed.contains(entry.note.id),
                onToggleCollapse: () {
                  setState(() {
                    if (_collapsed.contains(entry.note.id)) {
                      _collapsed.remove(entry.note.id);
                    } else {
                      _collapsed.add(entry.note.id);
                    }
                  });
                },
                onJumpToParent: () => _jumpToParent(entry.note.id),
              ),
          if (entries.isEmpty && item != null)
            NoteCard(
              appState: widget.appState,
              item: item,
              elevated: true,
              showRawFallback: true,
              rawActivity: widget.activity,
              autoExpandThread: true,
              autoExpandReplies: true,
            ),
          if (entries.isEmpty && item == null)
            NoteCard(
              appState: widget.appState,
              item: TimelineItem(
                activityId: (widget.activity['id'] as String?)?.trim() ?? '',
                activityType: (widget.activity['type'] as String?)?.trim() ?? '',
                actor: (widget.activity['actor'] as String?)?.trim() ?? '',
                note: Note(
                  id: '',
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
              rawActivity: widget.activity,
            ),
        ],
      ),
    );
  }

  List<_ThreadEntry> _flattenThread(String rootId) {
    final out = <_ThreadEntry>[];
    if (!_notes.containsKey(rootId)) return out;
    void walk(String id, int depth) {
      final note = _notes[id];
      if (note == null) return;
      out.add(_ThreadEntry(note: note, depth: depth));
      if (_collapsed.contains(id)) return;
      final kids = _children[id] ?? const [];
      final sorted = kids.toList()
        ..sort((a, b) {
          final da = DateTime.tryParse(_notes[a]?.published ?? '') ?? DateTime.fromMillisecondsSinceEpoch(0);
          final db = DateTime.tryParse(_notes[b]?.published ?? '') ?? DateTime.fromMillisecondsSinceEpoch(0);
          return da.compareTo(db);
        });
      for (final k in sorted) {
        walk(k, depth + 1);
      }
    }
    walk(rootId, 0);
    return out;
  }

  void _jumpToParent(String noteId) {
    final parent = _parentsById[noteId];
    if (parent == null) return;
    final key = _nodeKeys[parent];
    if (key == null) return;
    final ctx = key.currentContext;
    if (ctx == null) return;
    Scrollable.ensureVisible(ctx, duration: const Duration(milliseconds: 250), curve: Curves.easeOut);
  }
}

class _ThreadEntry {
  const _ThreadEntry({required this.note, required this.depth});

  final Note note;
  final int depth;
}

class _ThreadNodeRow extends StatelessWidget {
  const _ThreadNodeRow({
    super.key,
    required this.appState,
    required this.note,
    required this.depth,
    required this.isRoot,
    required this.isCurrent,
    required this.hasChildren,
    required this.collapsed,
    required this.onToggleCollapse,
    required this.onJumpToParent,
  });

  final AppState appState;
  final Note note;
  final int depth;
  final bool isRoot;
  final bool isCurrent;
  final bool hasChildren;
  final bool collapsed;
  final VoidCallback onToggleCollapse;
  final VoidCallback onJumpToParent;

  @override
  Widget build(BuildContext context) {
    final pad = (depth * 16).toDouble();
    return Padding(
      padding: EdgeInsets.only(left: pad),
      child: Card(
        color: isRoot ? Theme.of(context).colorScheme.primary.withAlpha(20) : null,
        child: Column(
          children: [
            Row(
              children: [
                if (hasChildren)
                  IconButton(
                    tooltip: collapsed ? 'Expand' : 'Collapse',
                    onPressed: onToggleCollapse,
                    icon: Icon(collapsed ? Icons.chevron_right : Icons.expand_more),
                  )
                else
                  const SizedBox(width: 40),
                if (note.inReplyTo.isNotEmpty)
                  IconButton(
                    tooltip: 'Jump to parent',
                    onPressed: onJumpToParent,
                    icon: const Icon(Icons.keyboard_arrow_up),
                  )
                else
                  const SizedBox(width: 40),
                Expanded(
                  child: NoteCard(
                    appState: appState,
                    item: TimelineItem(
                      activityId: note.id,
                      activityType: 'Create',
                      actor: note.attributedTo,
                      note: note,
                      boostedBy: '',
                      inReplyToPreview: null,
                      quotePreview: null,
                    ),
                    elevated: isCurrent,
                    showRawFallback: false,
                    autoExpandThread: false,
                    autoExpandReplies: false,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
