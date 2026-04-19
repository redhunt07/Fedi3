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

  test('normalizes timeline entries with direct objects and sorts by cursor', () {
    final items = normalizeRelayTimelineEntries([
      {
        'id': 'activity-2',
        'cursor': 200,
        'created_at_ms': 200,
        'object': {
          'id': 'https://relay.example/users/bob/statuses/2',
          'type': 'Note',
        },
      },
      {
        'id': 'activity-1',
        'cursor': 100,
        'created_at_ms': 100,
        'object': {
          'id': 'https://relay.example/users/alice/statuses/1',
          'type': 'Note',
        },
      },
    ]);

    expect(items, hasLength(2));
    expect(items.first['id'], 'activity-2');
    expect(items.first['fedi3RelaySync'], isTrue);
    expect((items.first['object'] as Map)['id'],
        'https://relay.example/users/bob/statuses/2');
  });

  test('normalizes relay notifications preserving latest event first', () {
    final notifications = normalizeRelayNotifications([
      {
        'event_id': 10,
        'notification_kind': 'mention',
        'activity_type': 'Create',
        'created_at_ms': 1000,
        'activity': {'id': 'https://relay.example/activities/10'},
      },
      {
        'event_id': 11,
        'notification_kind': 'reaction',
        'activity_type': 'Like',
        'created_at_ms': 1001,
        'payload': {'emoji': ':rocket:'},
      },
    ]);

    expect(notifications, hasLength(2));
    expect(notifications.first['event_id'], 11);
    expect((notifications.first['activity'] as Map)['emoji'], ':rocket:');
  });

  test('normalizes relay chat entries preserving delivery metadata', () {
    final chat = normalizeRelayChatEntries([
      {
        'event_id': 3,
        'thread_id': 'thread-a',
        'message_id': 'message-a',
        'sender_actor': 'https://relay.example/users/bob',
        'created_at_ms': 555,
        'delivery_state': 'delivered',
        'envelope': {'ciphertext_b64': 'xyz'},
      },
    ]);

    expect(chat, hasLength(1));
    expect(chat.first['event_id'], 3);
    expect(chat.first['thread_id'], 'thread-a');
    expect(chat.first['delivery_state'], 'delivered');
    expect((chat.first['envelope'] as Map).containsKey('ciphertext_b64'), isTrue);
  });

  test('derives dm and group thread metadata from relay chat entries', () {
    final threads = deriveRelayChatThreads(
      [
        {
          'event_id': 30,
          'thread_id': 'dm-thread',
          'message_id': 'dm-1',
          'sender_actor': 'https://relay.example/users/bob',
          'created_at_ms': 5000,
          'envelope': {'ciphertext_b64': 'aaa'},
        },
        {
          'event_id': 31,
          'thread_id': 'group-thread',
          'message_id': 'group-1',
          'sender_actor': 'https://relay.example/users/bob',
          'created_at_ms': 6000,
          'envelope': {
            'ciphertext_b64': 'bbb',
            'thread_title': 'Core Team',
          },
        },
        {
          'event_id': 32,
          'thread_id': 'group-thread',
          'message_id': 'group-2',
          'sender_actor': 'https://relay.example/users/carol',
          'created_at_ms': 7000,
          'envelope': {'ciphertext_b64': 'ccc'},
        },
      ],
      seenByThread: const {'group-thread': 0, 'dm-thread': 0},
      selfActor: 'https://relay.example/users/alice',
    );

    expect(threads, hasLength(2));
    expect(threads.first.threadId, 'group-thread');
    expect(threads.first.kind, 'group');
    expect(threads.first.title, 'Core Team');
    expect(threads.last.threadId, 'dm-thread');
    expect(threads.last.kind, 'dm');
    expect(threads.last.dmActor, 'https://relay.example/users/bob');
  });

  test('sorts unread threads before read threads', () {
    final threads = deriveRelayChatThreads(
      [
        {
          'event_id': 90,
          'thread_id': 'read-thread',
          'message_id': 'read-1',
          'sender_actor': 'https://relay.example/users/bob',
          'created_at_ms': 9000,
          'envelope': {'ciphertext_b64': 'read'},
        },
        {
          'event_id': 91,
          'thread_id': 'unread-thread',
          'message_id': 'unread-1',
          'sender_actor': 'https://relay.example/users/carol',
          'created_at_ms': 8000,
          'envelope': {'ciphertext_b64': 'unread'},
        },
      ],
      seenByThread: const {
        'read-thread': 9000,
        'unread-thread': 0,
      },
      selfActor: 'https://relay.example/users/alice',
    );

    expect(threads, hasLength(2));
    expect(threads.first.threadId, 'unread-thread');
    expect(threads.last.threadId, 'read-thread');
  });
}
