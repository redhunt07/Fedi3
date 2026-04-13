/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'chat_models.dart';

class SyncCursorState {
  const SyncCursorState({
    required this.events,
    required this.notifications,
    required this.chat,
    required this.timelineHome,
  });

  final int events;
  final int notifications;
  final int chat;
  final int timelineHome;

  static const empty =
      SyncCursorState(events: 0, notifications: 0, chat: 0, timelineHome: 0);

  SyncCursorState copyWith({
    int? events,
    int? notifications,
    int? chat,
    int? timelineHome,
  }) {
    return SyncCursorState(
      events: events ?? this.events,
      notifications: notifications ?? this.notifications,
      chat: chat ?? this.chat,
      timelineHome: timelineHome ?? this.timelineHome,
    );
  }

  factory SyncCursorState.fromJson(Map<String, dynamic> json) {
    return SyncCursorState(
      events: _readInt(json['events']),
      notifications: _readInt(json['notifications']),
      chat: _readInt(json['chat']),
      timelineHome: _readInt(json['timeline_home'] ?? json['timelineHome']),
    );
  }

  Map<String, dynamic> toJson() => {
        'events': events,
        'notifications': notifications,
        'chat': chat,
        'timeline_home': timelineHome,
      };
}

class RelaySyncBootstrapSnapshot {
  const RelaySyncBootstrapSnapshot({
    required this.generatedAtMs,
    required this.cursors,
    required this.events,
    required this.notifications,
    required this.chat,
    required this.timelineHome,
  });

  final int generatedAtMs;
  final SyncCursorState cursors;
  final List<Map<String, dynamic>> events;
  final List<Map<String, dynamic>> notifications;
  final List<Map<String, dynamic>> chat;
  final List<Map<String, dynamic>> timelineHome;

  factory RelaySyncBootstrapSnapshot.fromJson(Map<String, dynamic> json) {
    final timeline = (json['timeline'] is Map)
        ? (json['timeline'] as Map).cast<String, dynamic>()
        : const <String, dynamic>{};
    return RelaySyncBootstrapSnapshot(
      generatedAtMs: _readInt(json['generated_at_ms'] ?? json['generatedAtMs']),
      cursors: SyncCursorState.fromJson(
        (json['cursors'] is Map)
            ? (json['cursors'] as Map).cast<String, dynamic>()
            : const <String, dynamic>{},
      ),
      events: _readMapList(json['events']),
      notifications: normalizeRelayNotifications(json['notifications']),
      chat: normalizeRelayChatEntries(json['chat']),
      timelineHome: normalizeRelayTimelineEntries(timeline['home']),
    );
  }

  Map<String, dynamic> toJson() => {
        'generated_at_ms': generatedAtMs,
        'cursors': cursors.toJson(),
        'events': events,
        'notifications': notifications,
        'chat': chat,
        'timeline': {
          'home': timelineHome,
        },
      };
}

class RelaySyncStreamEvent {
  const RelaySyncStreamEvent({
    required this.eventId,
    required this.kind,
    required this.username,
    required this.tsMs,
    required this.payload,
  });

  final int eventId;
  final String kind;
  final String username;
  final int tsMs;
  final Map<String, dynamic> payload;

  factory RelaySyncStreamEvent.fromJson(Map<String, dynamic> json) {
    final payloadRaw = json['payload'];
    return RelaySyncStreamEvent(
      eventId: _readInt(json['event_id'] ?? json['eventId']),
      kind: (json['kind'] as String?)?.trim() ?? 'events',
      username: (json['username'] as String?)?.trim() ?? '',
      tsMs: _readInt(json['ts_ms'] ?? json['tsMs']),
      payload: payloadRaw is Map
          ? payloadRaw.cast<String, dynamic>()
          : <String, dynamic>{},
    );
  }
}

List<Map<String, dynamic>> normalizeRelayTimelineEntries(dynamic raw) {
  final out = <Map<String, dynamic>>[];
  for (final entry in _readMapList(raw)) {
    if (entry['object'] is Map) {
      out.add({
        ...entry,
        'cursor': _readInt(entry['cursor'] ?? entry['created_at_ms']),
        'created_at_ms':
            _readInt(entry['created_at_ms'] ?? entry['cursor']),
        'fedi3RelaySync': true,
      });
      continue;
    }
    final note = (entry['note'] is Map)
        ? (entry['note'] as Map).cast<String, dynamic>()
        : const <String, dynamic>{};
    if (note.isEmpty) continue;
    final createdAtMs =
        _readInt(entry['created_at_ms'] ?? entry['createdAtMs'] ?? entry['cursor']);
    final id = (note['id'] as String?)?.trim();
    out.add({
      'id': (id != null && id.isNotEmpty) ? id : 'relay-home-$createdAtMs',
      'type': 'Create',
      'actor': (note['attributedTo'] as String?)?.trim() ?? '',
      'object': note,
      'published': (note['published'] as String?)?.trim(),
      'created_at_ms': createdAtMs,
      'cursor': _readInt(entry['cursor'] ?? createdAtMs),
      'fedi3RelaySync': true,
    });
  }
  out.sort((a, b) => _timelineSortKey(b).compareTo(_timelineSortKey(a)));
  return out;
}

List<Map<String, dynamic>> normalizeRelayNotifications(dynamic raw) {
  final out = <Map<String, dynamic>>[];
  for (final entry in _readMapList(raw)) {
    if (entry['activity'] is Map) {
      out.add({
        'event_id': _readInt(entry['event_id'] ?? entry['eventId']),
        'notification_kind':
            (entry['notification_kind'] as String?)?.trim() ?? 'activity',
        'activity_type': (entry['activity_type'] as String?)?.trim() ?? '',
        'created_at_ms':
            _readInt(entry['created_at_ms'] ?? entry['createdAtMs']),
        'activity': (entry['activity'] as Map).cast<String, dynamic>(),
      });
      continue;
    }
    final payload = (entry['payload'] is Map)
        ? (entry['payload'] as Map).cast<String, dynamic>()
        : <String, dynamic>{'raw': entry['payload']};
    out.add({
      'event_id': _readInt(entry['event_id'] ?? entry['eventId']),
      'notification_kind':
          (entry['notification_kind'] as String?)?.trim() ?? 'activity',
      'activity_type': (entry['activity_type'] as String?)?.trim() ?? '',
      'created_at_ms': _readInt(entry['created_at_ms'] ?? entry['createdAtMs']),
      'activity': payload,
    });
  }
  out.sort((a, b) =>
      _readInt(b['event_id']).compareTo(_readInt(a['event_id'])));
  return out;
}

List<Map<String, dynamic>> normalizeRelayChatEntries(dynamic raw) {
  final out = <Map<String, dynamic>>[];
  for (final entry in _readMapList(raw)) {
    final envelope = (entry['envelope'] is Map)
        ? (entry['envelope'] as Map).cast<String, dynamic>()
        : <String, dynamic>{'raw': entry['envelope']};
    out.add({
      'event_id': _readInt(entry['event_id'] ?? entry['eventId']),
      'thread_id': (entry['thread_id'] as String?)?.trim() ?? '',
      'message_id': (entry['message_id'] as String?)?.trim() ?? '',
      'sender_actor': (entry['sender_actor'] as String?)?.trim() ?? '',
      'sender_user': (entry['sender_user'] as String?)?.trim(),
      'created_at_ms': _readInt(entry['created_at_ms'] ?? entry['createdAtMs']),
      'delivery_state': (entry['delivery_state'] as String?)?.trim() ?? 'stored',
      'envelope': envelope,
    });
  }
  out.sort((a, b) =>
      _readInt(b['event_id']).compareTo(_readInt(a['event_id'])));
  return out;
}

List<ChatThreadItem> deriveRelayChatThreads(
  List<Map<String, dynamic>> chatEntries, {
  required Map<String, int> seenByThread,
  String? selfActor,
}) {
  final grouped = <String, List<Map<String, dynamic>>>{};
  for (final entry in chatEntries) {
    final threadId = (entry['thread_id'] as String?)?.trim() ?? '';
    if (threadId.isEmpty) continue;
    grouped.putIfAbsent(threadId, () => <Map<String, dynamic>>[]).add(entry);
  }
  final items = <ChatThreadItem>[];
  for (final group in grouped.entries) {
    final rows = List<Map<String, dynamic>>.from(group.value)
      ..sort((a, b) =>
          _readInt(b['created_at_ms']).compareTo(_readInt(a['created_at_ms'])));
    final latest = rows.first;
    final preview = _relayChatPreview(latest['envelope']);
    final senders = rows
        .map((row) => (row['sender_actor'] as String?)?.trim() ?? '')
        .where((value) => value.isNotEmpty)
        .toSet();
    final dmCandidate = senders.where((value) => value != (selfActor ?? '')).toList();
    final isDm = dmCandidate.length == 1;
    final updatedAtMs = _readInt(latest['created_at_ms']);
    final title = isDm
        ? null
        : (((latest['envelope'] is Map)
                    ? (latest['envelope'] as Map)['thread_title']
                    : null)
                ?.toString()
                .trim()
                .isNotEmpty ==
            true)
            ? ((latest['envelope'] as Map)['thread_title'] as String).trim()
            : 'Encrypted thread';
    items.add(
      ChatThreadItem(
        threadId: group.key,
        kind: isDm ? 'dm' : 'group',
        title: title,
        createdAtMs: rows
            .map((row) => _readInt(row['created_at_ms']))
            .fold<int>(updatedAtMs, (min, value) => value < min ? value : min),
        updatedAtMs: updatedAtMs,
        lastMessageMs: updatedAtMs,
        lastMessagePreview: preview,
        dmActor: isDm ? dmCandidate.first : null,
      ),
    );
  }
  items.sort((a, b) {
    final aSeen = seenByThread[a.threadId] ?? 0;
    final bSeen = seenByThread[b.threadId] ?? 0;
    final aUnread = (a.lastMessageMs ?? a.updatedAtMs) > aSeen;
    final bUnread = (b.lastMessageMs ?? b.updatedAtMs) > bSeen;
    if (aUnread != bUnread) return aUnread ? -1 : 1;
    return (b.lastMessageMs ?? b.updatedAtMs)
        .compareTo(a.lastMessageMs ?? a.updatedAtMs);
  });
  return items;
}

String _relayChatPreview(dynamic rawEnvelope) {
  if (rawEnvelope is Map) {
    final envelope = rawEnvelope.cast<String, dynamic>();
    final raw = envelope['raw']?.toString().trim() ?? '';
    final text = envelope['text']?.toString().trim() ?? '';
    if (text.isNotEmpty) return text;
    if (raw.isNotEmpty) return 'Encrypted message';
    if (envelope['ciphertext_b64'] != null) return 'Encrypted message';
  }
  return 'Encrypted message';
}

int _timelineSortKey(Map<String, dynamic> item) {
  final created = _readInt(item['created_at_ms'] ?? item['cursor']);
  if (created > 0) return created;
  final published = (item['published'] as String?)?.trim() ?? '';
  final parsed = published.isEmpty ? null : DateTime.tryParse(published);
  return parsed?.millisecondsSinceEpoch ?? 0;
}

List<Map<String, dynamic>> _readMapList(dynamic raw) {
  if (raw is! List) return const [];
  return raw
      .whereType<Map>()
      .map((value) => value.cast<String, dynamic>())
      .toList();
}

int _readInt(dynamic raw) {
  if (raw is num) return raw.toInt();
  if (raw is String) return int.tryParse(raw.trim()) ?? 0;
  return 0;
}
