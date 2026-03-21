/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:collection';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../model/note_models.dart';
import '../../services/object_repository.dart';
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
  static final LinkedHashMap<String, Map<String, dynamic>> _hydratedCache =
      LinkedHashMap<String, Map<String, dynamic>>();
  static final Map<String, Future<Map<String, dynamic>?>> _inflight =
      <String, Future<Map<String, dynamic>?>>{};
  static const int _maxHydratedCacheEntries = 512;

  Map<String, dynamic>? _activity;
  String _fingerprint = '';

  @override
  void initState() {
    super.initState();
    _activity = widget.activity;
    _fingerprint = _activityFingerprint(widget.activity);
    _hydrateIfNeeded();
  }

  @override
  void didUpdateWidget(covariant TimelineActivityCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    final nextFingerprint = _activityFingerprint(widget.activity);
    if (nextFingerprint != _fingerprint) {
      _fingerprint = nextFingerprint;
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

    final cached = _hydratedCache[url];
    if (cached != null) {
      _remember(url, cached);
      if (!mounted) return;
      setState(() {
        _activity = {...a, 'object': cached};
      });
      return;
    }

    try {
      final inflight = _inflight[url];
      final resolved = inflight ??
          _resolveObject(url, widget.appState.config!);
      if (inflight == null) {
        _inflight[url] = resolved;
      }
      Map<String, dynamic>? fetched;
      try {
        fetched = await resolved;
      } finally {
        if (inflight == null) {
          _inflight.remove(url);
        }
      }
      if (!mounted) return;
      if (fetched == null) return;
      _remember(url, fetched);
      setState(() {
        _activity = {...a, 'object': fetched};
      });
    } catch (_) {
      // best-effort
    }
  }

  Future<Map<String, dynamic>?> _resolveObject(String url, config) async {
    final api = CoreApi(config: config);
    var resolved = await api.fetchCachedObject(url);
    resolved ??= await ObjectRepository.instance.fetchObject(url);
    return resolved;
  }

  void _remember(String key, Map<String, dynamic> value) {
    _hydratedCache.remove(key);
    _hydratedCache[key] = value;
    while (_hydratedCache.length > _maxHydratedCacheEntries) {
      _hydratedCache.remove(_hydratedCache.keys.first);
    }
  }

  String _activityFingerprint(Map<String, dynamic> activity) {
    final id = (activity['id'] as String?)?.trim() ?? '';
    final type = (activity['type'] as String?)?.trim() ?? '';
    final object = activity['object'];
    if (object is String) {
      return '$id|$type|${object.trim()}';
    }
    if (object is Map) {
      final map = object.cast<String, dynamic>();
      final objectId = (map['id'] as String?)?.trim() ?? '';
      final objectUpdated = (map['updated'] as String?)?.trim() ?? '';
      final objectPublished = (map['published'] as String?)?.trim() ?? '';
      return '$id|$type|$objectId|$objectUpdated|$objectPublished';
    }
    return '$id|$type';
  }

  @override
  Widget build(BuildContext context) {
    final a = _activity ?? widget.activity;
    final item = TimelineItem.tryFromActivity(a);

    if (item == null) {
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
