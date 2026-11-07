/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/model/note_models.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/widgets/note_card.dart';
import 'package:fedi3/ui/widgets/status_avatar.dart';
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

TimelineItem _item({String content = '<p>hello world</p>'}) {
  final note = Note.tryParse({
    'id': 'https://other.test/notes/1',
    'type': 'Note',
    'attributedTo': 'https://other.test/users/bob',
    'content': content,
    'published': '2026-10-31T10:00:00Z',
  })!;
  return TimelineItem(
    activityId: 'https://other.test/activities/1',
    activityType: 'Create',
    actor: 'https://other.test/users/bob',
    note: note,
    boostedBy: '',
    inReplyToPreview: null,
    quotePreview: null,
  );
}

void main() {
  testWidgets('a normal note opens external links through the URL policy',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);
    final opened = <String>[];
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    const linuxLaunch = BasicMessageChannel<Object?>(
        'dev.flutter.pigeon.url_launcher_linux.UrlLauncherApi.launchUrl',
        StandardMessageCodec());
    messenger.setMockDecodedMessageHandler(linuxLaunch, (message) async {
      if (message is List && message.first is String) {
        opened.add(message.first as String);
      }
      return <Object?>[null];
    });
    addTearDown(
        () => messenger.setMockDecodedMessageHandler(linuxLaunch, null));
    for (final channel in [
      'plugins.flutter.io/url_launcher',
      'plugins.flutter.io/url_launcher_linux'
    ]) {
      messenger.setMockMethodCallHandler(MethodChannel(channel), (call) async {
        if (call.arguments is Map) {
          final url = (call.arguments as Map)['url'];
          if (url is String) opened.add(url);
        }
        return true;
      });
      addTearDown(() =>
          messenger.setMockMethodCallHandler(MethodChannel(channel), null));
    }
    final app = await _runningApp();
    await pumpScreen(
        tester,
        NoteCard(
            appState: app,
            item: _item(
                content:
                    '<p><a href="https://external.example/article">External link</a></p>')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    for (var attempt = 0;
        attempt < 50 &&
            find.text('External link', findRichText: true).evaluate().isEmpty;
        attempt++) {
      await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 20)));
      await tester.pump();
    }
    await tester.tapAt(
        tester.getTopLeft(find.text('External link', findRichText: true)) +
            const Offset(12, 8));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(opened, contains('https://external.example/article'));
  });
  testWidgets('AW-08 note card like triggers the correct outbox call',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient(
      defaultResponse: FakeHttpResponse(body: '{}'),
    );
    client.json('GET', '/_fedi3/reactions/me', {'items': <dynamic>[]});
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, NoteCard(appState: app, item: _item()));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.byType(NoteCard), findsOneWidget);

    await tester.tap(find.byIcon(Icons.favorite_border));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    final outbox = client.requestsFor('POST', '/users/alice/outbox');
    expect(outbox, hasLength(1));
    final likeIndex =
        client.requests.indexWhere((r) => r.url.path == '/users/alice/outbox');
    final payload =
        jsonDecode(client.requestBodies[likeIndex]) as Map<String, dynamic>;
    expect(payload['type'], 'Like');
    expect(payload['object'], 'https://other.test/notes/1');
    expect(payload['to'], ['https://other.test/users/bob']);
    expect(payload['actor'], 'https://example.test/users/alice');
  });

  testWidgets('AW-08b note card boost sends an Announce', (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/_fedi3/reactions/me', {'items': <dynamic>[]});
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, NoteCard(appState: app, item: _item()));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    await tester.tap(find.byIcon(Icons.repeat));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    final announceIndex =
        client.requests.indexWhere((r) => r.url.path == '/users/alice/outbox');
    final payload =
        jsonDecode(client.requestBodies[announceIndex]) as Map<String, dynamic>;
    expect(payload['type'], 'Announce');
    expect(payload['object'], 'https://other.test/notes/1');
  });

  testWidgets('AW-14 avatar exposes a semantic label', (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);

    await pumpScreen(
      tester,
      const Scaffold(
        body: StatusAvatar(
          imageUrl: 'http://127.0.0.1:8788/media/avatar.png',
          size: 40,
        ),
      ),
    );
    await tester.pump();

    final image = tester.widget<Image>(find.byType(Image));
    expect(image.semanticLabel, 'Avatar');
  });
}
