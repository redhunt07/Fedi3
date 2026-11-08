/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/chat_search_screen.dart';
import 'package:fedi3/ui/screens/chat_thread_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

Future<AppState> _runningApp() async {
  final app =
      buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
  await app.startCore();
  return app;
}

void main() {
  testWidgets('failed chat delivery preserves the complete draft for retry',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/chat/threads/t1', {'items': [], 'next': null});
    client.route('POST', '/_fedi3/chat/send',
        FakeHttpResponse(statusCode: 503, body: 'offline'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app = await _runningApp();
    await pumpScreen(tester,
        ChatThreadScreen(appState: app, threadId: 't1', title: 'Thread'));
    await tester.pumpAndSettle();
    const draft =
        '  A long message that must survive a network error.\nSecond line.  ';
    final field = find.byType(TextField).last;
    await tester.enterText(field, draft);
    await tester.tap(find.byIcon(Icons.send_rounded));
    await tester.pumpAndSettle();
    expect(tester.widget<TextField>(field).controller!.text, draft);
    client.json('POST', '/_fedi3/chat/send', {'message_id': 'sent'});
    await tester.tap(find.byIcon(Icons.send_rounded));
    await tester.pumpAndSettle();
    expect(tester.widget<TextField>(field).controller!.text, isEmpty);
  });
  testWidgets('AW-09 chat sends the typed message with the correct payload',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json(
        'GET', '/_fedi3/chat/threads/t1', {'items': <dynamic>[], 'next': null});
    client
        .json('POST', '/_fedi3/chat/status', {'statuses': <String, dynamic>{}});
    client.json('POST', '/_fedi3/chat/reactions', {'items': <dynamic>[]});
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      ChatThreadScreen(appState: app, threadId: 't1', title: 'Thread'),
    );
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField).last, 'hello from the test');
    await tester.pump();
    await tester.tap(find.byIcon(Icons.send_rounded));
    await tester.pumpAndSettle();

    final sends = client.requestsFor('POST', '/_fedi3/chat/send');
    expect(sends, hasLength(1));
    final index =
        client.requests.indexWhere((r) => r.url.path == '/_fedi3/chat/send');
    final payload =
        jsonDecode(client.requestBodies[index]) as Map<String, dynamic>;
    expect(payload['thread_id'], 't1');
    expect(payload['text'], 'hello from the test');
  });

  testWidgets('AW-09f chat sends on Enter and inserts newline on Alt+Enter',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json(
        'GET', '/_fedi3/chat/threads/t1', {'items': <dynamic>[], 'next': null});
    client
        .json('POST', '/_fedi3/chat/status', {'statuses': <String, dynamic>{}});
    client.json('POST', '/_fedi3/chat/reactions', {'items': <dynamic>[]});
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      ChatThreadScreen(appState: app, threadId: 't1', title: 'Thread'),
    );
    await tester.pumpAndSettle();

    final field = find.byType(TextField).last;
    await tester.enterText(field, 'sent with enter');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(client.requestsFor('POST', '/_fedi3/chat/send'), hasLength(1));
    final composer = tester.widget<TextField>(field);
    expect(composer.focusNode?.hasFocus, isTrue);

    await tester.enterText(field, 'first line');
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pump();

    final controller = tester.widget<TextField>(field).controller!;
    expect(controller.text, contains('\n'));
    expect(client.requestsFor('POST', '/_fedi3/chat/send'), hasLength(1));
  });

  testWidgets('AW-09b chat search filters loaded messages', (tester) async {
    await setUpPumpEnv(tester);
    Map<String, dynamic> message(String id, String text, int at) => {
          'message_id': id,
          'thread_id': 't1',
          'sender_actor': 'https://fedi3.test/users/alice',
          'sender_device': 'dev',
          'created_at_ms': at,
          'edited_at_ms': null,
          'deleted': false,
          'body_json': jsonEncode({'op': 'message', 'text': text}),
        };
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/chat/threads/t1', {
      'items': [
        message('m1', 'prima notizia', 1000),
        message('m2', 'seconda notizia', 2000),
      ],
      'next': null,
    });
    client.json('GET', '/_fedi3/chat/thread/search', {
      'items': [message('m2', 'seconda notizia', 2000)],
    });
    client
        .json('POST', '/_fedi3/chat/status', {'statuses': <String, dynamic>{}});
    client.json('POST', '/_fedi3/chat/reactions', {'items': <dynamic>[]});
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      ChatThreadScreen(appState: app, threadId: 't1', title: 'Thread'),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byType(PopupMenuButton<String>));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Search messages'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).first, 'seconda');
    await tester.pumpAndSettle();

    expect(find.text('seconda notizia'), findsOneWidget);
    expect(find.text('prima notizia'), findsNothing);
  });

  testWidgets('AW-09c chat forwards a message to another thread',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/chat/threads/t1', {
      'items': [
        {
          'message_id': 'm1',
          'thread_id': 't1',
          'sender_actor': 'https://fedi3.test/users/alice',
          'sender_device': 'dev',
          'created_at_ms': 1000,
          'edited_at_ms': null,
          'deleted': false,
          'body_json': jsonEncode({'op': 'message', 'text': 'da inoltrare'}),
        }
      ],
      'next': null,
    });
    client.json('GET', '/_fedi3/chat/threads', {
      'items': [
        {
          'thread_id': 't2',
          'kind': 'dm',
          'title': 'Destinazione',
          'created_at_ms': 1,
          'updated_at_ms': 2,
          'peer_usernames': ['bob'],
          'last_message_preview': null,
          'last_message_ms': null,
        }
      ],
      'next': null,
    });
    client
        .json('POST', '/_fedi3/chat/status', {'statuses': <String, dynamic>{}});
    client.json('POST', '/_fedi3/chat/reactions', {'items': <dynamic>[]});
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      ChatThreadScreen(appState: app, threadId: 't1', title: 'Thread'),
    );
    await tester.pumpAndSettle();

    await tester.longPress(find.text('da inoltrare'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Forward'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Destinazione'));
    await tester.pumpAndSettle();

    final sends = client.requestsFor('POST', '/_fedi3/chat/send');
    expect(sends, hasLength(1));
    final index =
        client.requests.indexWhere((r) => r.url.path == '/_fedi3/chat/send');
    final payload =
        jsonDecode(client.requestBodies[index]) as Map<String, dynamic>;
    expect(payload['thread_id'], 't2');
    expect(payload['text'], 'da inoltrare');
  });

  testWidgets('AW-09d chat shows the pinned message banner', (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/chat/threads/t1', {
      'items': <dynamic>[],
      'next': null,
    });
    client.json('GET', '/_fedi3/chat/thread/members', {
      'items': [
        {'actor_id': 'https://fedi3.test/users/alice', 'role': 'owner'}
      ]
    });
    client.json('GET', '/_fedi3/chat/thread/pin', {
      'pin': {
        'message_id': 'm1',
        'pinned_by': 'https://fedi3.test/users/alice',
        'pinned_at_ms': 1,
        'preview': 'messaggio importante',
      }
    });
    client
        .json('POST', '/_fedi3/chat/status', {'statuses': <String, dynamic>{}});
    client.json('POST', '/_fedi3/chat/reactions', {'items': <dynamic>[]});
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      ChatThreadScreen(
        appState: app,
        threadId: 't1',
        title: 'Team',
        isGroup: true,
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('PINNED MESSAGE'), findsOneWidget);
    expect(find.text('messaggio importante'), findsOneWidget);
  });

  testWidgets('AW-09e global chat search lists membership-scoped hits',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/chat/search', {
      'items': [
        {
          'message_id': 'm1',
          'thread_id': 't1',
          'thread_kind': 'group',
          'thread_title': 'Team',
          'sender_actor': 'https://a.test/users/bob',
          'created_at_ms': 1000,
          'text': 'ciao a tutti',
        }
      ]
    });
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, ChatSearchScreen(appState: app));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), 'ciao');
    await tester.pump(const Duration(milliseconds: 350));
    await tester.pumpAndSettle();

    expect(find.text('Team'), findsOneWidget);
    expect(find.textContaining('ciao a tutti'), findsOneWidget);
    expect(find.textContaining('@bob'), findsOneWidget);
  });
}
