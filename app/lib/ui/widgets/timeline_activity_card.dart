/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/note_models.dart';
import '../../state/app_state.dart';
import 'note_card.dart';

class TimelineActivityCard extends StatefulWidget {
  const TimelineActivityCard({
    super.key,
    required this.appState,
    required this.activity,
    this.elevated = false,
  });

  final AppState appState;
  final Map<String, dynamic> activity;
  final bool elevated;

  @override
  State<TimelineActivityCard> createState() => _TimelineActivityCardState();
}

class _TimelineActivityCardState extends State<TimelineActivityCard> {
  Map<String, dynamic>? _activity;
  bool _hydrating = false;

  @override
  void initState() {
    super.initState();
    _activity = widget.activity;
    _hydrateIfNeeded();
  }

  @override
  void didUpdateWidget(covariant TimelineActivityCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.activity != widget.activity) {
      _activity = widget.activity;
      _hydrateIfNeeded();
    }
  }

  Future<void> _hydrateIfNeeded() async {
    final a = _activity;
    if (a == null) return;
    final obj = a['object'];
    if (obj is! String) return;
    final url = obj.trim();
    if (url.isEmpty) return;

    setState(() => _hydrating = true);
    try {
      final api = CoreApi(config: widget.appState.config!);
      final cached = await api.fetchCachedObject(url);
      if (!mounted) return;
      if (cached == null) return;
      setState(() {
        _activity = {...a, 'object': cached};
      });
    } catch (_) {
      // best-effort
    } finally {
      if (mounted) setState(() => _hydrating = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final a = _activity ?? widget.activity;
    final item = TimelineItem.tryFromActivity(a);

    if (item == null) {
      final type = (a['type'] as String?)?.trim() ?? '';
      final actor = (a['actor'] as String?)?.trim() ?? '';
      final obj = a['object'];
      final objType = (obj is Map) ? (obj['type'] as String?)?.trim() ?? '' : '';
      var objId = '';
      if (obj is String) {
        objId = obj.trim();
      } else if (obj is Map) {
        objId = (obj['id'] as String?)?.trim() ?? (obj['url'] as String?)?.trim() ?? '';
      }
      return Card(
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Row(
            children: [
              if (_hydrating) const SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)),
              if (_hydrating) const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      type.isNotEmpty ? type : context.l10n.activityUnsupported,
                      style: const TextStyle(fontWeight: FontWeight.w700),
                    ),
                    if (objType.isNotEmpty) Text(objType, style: const TextStyle(fontSize: 12)),
                    if (actor.isNotEmpty) Text(actor, style: const TextStyle(fontSize: 12)),
                    if (objId.isNotEmpty) Text(objId, style: const TextStyle(fontSize: 12)),
                    Align(
                      alignment: Alignment.centerLeft,
                      child: TextButton(
                        onPressed: () {
                          final raw = const JsonEncoder.withIndent('  ').convert(a);
                          showDialog<void>(
                            context: context,
                            builder: (context) => AlertDialog(
                              title: Text(context.l10n.activityViewRaw),
                              content: SizedBox(
                                width: 520,
                                child: SingleChildScrollView(child: SelectableText(raw)),
                              ),
                            ),
                          );
                        },
                        child: Text(context.l10n.activityViewRaw),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      );
    }

    return RepaintBoundary(
      child: NoteCard(
        appState: widget.appState,
        item: item,
        elevated: widget.elevated,
        showRawFallback: true,
        rawActivity: a,
      ),
    );
  }
}
