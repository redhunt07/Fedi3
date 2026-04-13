/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter_test/flutter_test.dart';

import 'package:fedi3/model/relay_sync_models.dart';

void main() {
  test('normalizes relay timeline bootstrap entries into activity-like maps', () {
    final items = normalizeRelayTimelineEntries([
      {
        'cursor': 42,
        'created_at_ms': 42,
        'note': {
          'id': 'https://relay.example/users/alice/statuses/1',
          'type': 'Note',
          'attributedTo': 'https://relay.example/users/alice',
          'content': '<p>Hello</p>',
          'published': '2026-04-11T10:00:00Z',
        },
      },
    ]);

    expect(items, hasLength(1));
    expect(items.first['type'], 'Create');
    expect(items.first['fedi3RelaySync'], isTrue);
    expect((items.first['object'] as Map)['id'],
        'https://relay.example/users/alice/statuses/1');
  });

  test('derives encrypted relay chat threads ordered by unread first', () {
    final threads = deriveRelayChatThreads(
      [
        {
          'event_id': 5,
          'thread_id': 'thread-a',
          'message_id': 'msg-a',
          'sender_actor': 'https://relay.example/users/bob',
          'created_at_ms': 200,
          'envelope': {'ciphertext_b64': 'abc'},
        },
        {
          'event_id': 4,
          'thread_id': 'thread-b',
          'message_id': 'msg-b',
          'sender_actor': 'https://relay.example/users/carol',
          'created_at_ms': 100,
          'envelope': {'ciphertext_b64': 'def'},
        },
      ],
      seenByThread: const {'thread-b': 100},
      selfActor: 'https://relay.example/users/alice',
    );

    expect(threads, hasLength(2));
    expect(threads.first.threadId, 'thread-a');
    expect(threads.first.lastMessagePreview, 'Encrypted message');
  });
}
