/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/chat_screen.dart';
import 'package:fedi3/ui/screens/chat_search_screen.dart';
import 'package:fedi3/ui/screens/chat_thread_screen.dart';
import 'package:fedi3/ui/screens/profile_screen.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:media_kit/media_kit.dart';

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

/// Minimal audioplayers platform mock: the voice-note tile must reach its
/// ready state without a native plugin in widget tests.
void _mockAudioPlugin() {
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  messenger.setMockMethodCallHandler(
    const MethodChannel('xyz.luan/audioplayers.global'),
    (call) async => null,
  );
  messenger.setMockMethodCallHandler(
    const MethodChannel('xyz.luan/audioplayers'),
    (call) async {
      if (call.method == 'create') {
        final args = call.arguments is Map ? call.arguments as Map : const {};
        final playerId = args['playerId']?.toString() ?? '';
        messenger.setMockStreamHandler(
          EventChannel('xyz.luan/audioplayers/events/$playerId'),
          MockStreamHandler.inline(
            onListen: (arguments, sink) {
              sink.success(<String, dynamic>{
                'event': 'audio.onPrepared',
                'value': true,
              });
            },
          ),
        );
      }
      if (call.method == 'getDuration') return 3000;
      if (call.method == 'getCurrentPosition') return 0;
      return null;
    },
  );
}

Map<String, dynamic> _attachmentMessage({
  required String attachmentId,
  required String url,
  required String mediaType,
  required String name,
}) {
  return {
    'message_id': 'm-$attachmentId',
    'thread_id': 't1',
    'sender_actor': 'https://fedi3.test/users/bob',
    'sender_device': 'dev',
    'created_at_ms': 1000,
    'edited_at_ms': null,
    'deleted': false,
    'body_json': jsonEncode({
      'op': 'message',
      'text': '',
      'attachments': [
        {
          'id': attachmentId,
          'url': url,
          'mediaType': mediaType,
          'name': name,
        }
      ],
    }),
  };
}

FakeHttpClient _attachmentClient(Map<String, dynamic> message) {
  final client = FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
  client.json('GET', '/_fedi3/chat/threads/t1', {
    'items': [message],
    'next': null,
  });
  client.json('POST', '/_fedi3/chat/status', {'statuses': <String, dynamic>{}});
  client.json('POST', '/_fedi3/chat/reactions', {'items': <dynamic>[]});
  client.route(
    'GET',
    '/_fedi3/stream',
    FakeHttpResponse(
        headers: const {'content-type': 'text/event-stream'}, body: ''),
  );
  return client;
}

void main() {
  testWidgets(
      'composer stays above Android navigation and recovers missed messages',
      (tester) async {
    await setUpPumpEnv(tester);
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(360, 760);
    tester.view.padding = const FakeViewPadding(bottom: 48);
    tester.view.viewPadding = const FakeViewPadding(bottom: 48);
    addTearDown(() {
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
      tester.view.resetPadding();
      tester.view.resetViewPadding();
    });
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{"items":[]}'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app = await _runningApp();
    await pumpScreen(tester,
        ChatThreadScreen(appState: app, threadId: 'safe-chat', title: 'Chat'));
    await tester.pumpAndSettle();
    expect(tester.getBottomRight(find.byIcon(Icons.send_rounded)).dy,
        lessThanOrEqualTo(712));
    final before =
        client.requestsFor('GET', '/_fedi3/chat/threads/safe-chat').length;
    await tester.pump(const Duration(seconds: 5));
    await tester.pumpAndSettle();
    expect(client.requestsFor('GET', '/_fedi3/chat/threads/safe-chat').length,
        greaterThan(before));
    await tester.pumpWidget(const SizedBox());
  });

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

  testWidgets(
      'audio note with an empty mediaType is still a playable voice bubble',
      (tester) async {
    await setUpPumpEnv(tester);
    _mockAudioPlugin();
    final client = _attachmentClient(_attachmentMessage(
      attachmentId: 'a1',
      url: 'https://fedi3.test/media/note.m4a',
      mediaType: '',
      name: 'note.m4a',
    ));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      ChatThreadScreen(appState: app, threadId: 't1', title: 'Thread'),
    );
    await tester.pumpAndSettle();

    // The recipient sees the existing voice-note bubble, not a plain file row.
    expect(find.byIcon(Icons.play_arrow_rounded), findsOneWidget);
    expect(find.byIcon(Icons.insert_drive_file), findsNothing);
  });

  testWidgets(
      'video/mp4 attachment keeps the inline tile with the play overlay',
      (tester) async {
    await setUpPumpEnv(tester);
    // The inline tile renders a first-frame poster through media_kit.
    MediaKit.ensureInitialized();
    final client = _attachmentClient(_attachmentMessage(
      attachmentId: 'v1',
      url: 'http://127.0.0.1:1/media/clip.mp4',
      mediaType: 'video/mp4',
      name: 'clip.mp4',
    ));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      ChatThreadScreen(appState: app, threadId: 't1', title: 'Thread'),
    );
    await tester.pumpAndSettle();
    await tester.pump(const Duration(milliseconds: 400));

    expect(find.byIcon(Icons.play_circle_fill), findsOneWidget);
    expect(find.byIcon(Icons.insert_drive_file), findsNothing);
  });

  testWidgets('chat list shows the DM peer nickname instead of the @handle',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/chat/threads', {
      'items': [
        {
          'thread_id': 'dm1',
          'kind': 'dm',
          'title': null,
          'created_at_ms': 1,
          'updated_at_ms': 2,
          'last_message_ms': 2,
          'last_message_preview': 'ciao',
          'peer_usernames': ['bob'],
          'peer_actors': ['https://example.test/users/bob'],
        }
      ],
      'next': null,
    });
    client.json('GET', '/users/bob', {
      'type': 'Person',
      'id': 'https://example.test/users/bob',
      'preferredUsername': 'bob',
      'name': 'Bob Rossi',
    });
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, ChatScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('Bob Rossi'), findsOneWidget);
    expect(find.text('@bob'), findsNothing);
  });

  testWidgets('tapping the DM header nickname opens the peer profile',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/chat/threads/t1', {
      'items': <dynamic>[],
      'next': null,
    });
    client.json('GET', '/_fedi3/chat/thread/members', {
      'items': [
        {
          'actor_id': 'https://example.test/users/bob',
          'role': 'member',
        }
      ],
    });
    client.json('GET', '/users/bob', {
      'type': 'Person',
      'id': 'https://example.test/users/bob',
      'preferredUsername': 'bob',
      'name': 'Bob Rossi',
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

    await tester.tap(find.text('Bob Rossi'));
    await tester.pumpAndSettle();

    expect(find.byType(ProfileScreen), findsOneWidget);
  });
}
