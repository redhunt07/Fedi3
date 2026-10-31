/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/chat_thread_screen.dart';
import 'package:flutter/material.dart';
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
    await tester.tap(find.byIcon(Icons.send));
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
}
