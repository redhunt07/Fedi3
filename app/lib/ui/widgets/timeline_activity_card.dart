/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
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
      if (_hydrating) {
        return const Padding(
          padding: EdgeInsets.all(12),
          child: SizedBox(
              width: 18,
              height: 18,
              child: CircularProgressIndicator(strokeWidth: 2)),
        );
      }
      assert(() {
        final compact = {
          'type': a['type'],
          'id': a['id'],
          'actor': a['actor'],
        };
        debugPrint(
            'timeline drop unsupported activity: ${const JsonEncoder().convert(compact)}');
        return true;
      }());
      return const SizedBox.shrink();
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
