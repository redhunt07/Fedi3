/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/compose_screen.dart';
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
  testWidgets('AW-06 compose button is disabled while the text is empty',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, ComposeScreen(appState: app));
    await tester.pumpAndSettle();

    final postButton = find.widgetWithText(FilledButton, 'Post');
    expect(postButton, findsOneWidget);
    expect(tester.widget<FilledButton>(postButton).onPressed, isNull);

    await tester.enterText(find.byType(TextField).first, 'hello');
    await tester.pump();
    expect(tester.widget<FilledButton>(postButton).onPressed, isNotNull);
  });

  testWidgets('AW-07 compose sends escaped HTML content to the outbox',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, ComposeScreen(appState: app));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField).first, '1 < 2 & 3');
    await tester.pump();
    await tester.tap(find.widgetWithText(FilledButton, 'Post'));
    await tester.pumpAndSettle();

    final outbox = client.requestsFor('POST', '/users/alice/outbox');
    expect(outbox, hasLength(1));
    final payload =
        jsonDecode(client.requestBodies.last) as Map<String, dynamic>;
    final content =
        (payload['object'] as Map<String, dynamic>)['content'] as String;
    // D4/H-62: content is always HTML, escaped even without MFM markers.
    expect(content, startsWith('<p>'));
    expect(content, contains('&lt;'));
    expect(content, contains('&amp;'));
    expect(content, isNot(contains('< 2')));
  });
}
