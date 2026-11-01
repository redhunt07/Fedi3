/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/onboarding_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  testWidgets('AW-02 onboarding rejects an empty username with an inline error',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: null);
    await pumpScreen(tester,
        OnboardingScreen(appState: app, mode: OnboardingMode.createNew));
    await tester.pumpAndSettle();

    final usernameField = find.widgetWithText(TextField, 'Username');
    expect(usernameField, findsOneWidget);
    await tester.enterText(usernameField, '');
    await tester.pump();

    await tester.scrollUntilVisible(find.text('Save & open app'), 200,
        scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Save & open app'));
    await tester.pumpAndSettle();

    expect(
      find.text(
          'Username must be 1-64 characters: letters, digits or underscore.'),
      findsOneWidget,
    );
    expect(app.config, isNull);
  });

  testWidgets('AW-02b onboarding rejects a malformed public URL',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: null);
    await pumpScreen(tester,
        OnboardingScreen(appState: app, mode: OnboardingMode.createNew));
    await tester.pumpAndSettle();

    await tester.scrollUntilVisible(find.text('Use custom relay settings'), 200,
        scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Use custom relay settings'));
    await tester.pumpAndSettle();

    final urlField = find.widgetWithText(
      TextField,
      'Relay public URL (https://relay... or http://127.0.0.1:8787)',
    );
    await tester.scrollUntilVisible(urlField, 200,
        scrollable: find.byType(Scrollable).first);
    await tester.enterText(urlField, 'not-a-url');
    await tester.pump();
    await tester.scrollUntilVisible(find.text('Save & open app'), 200,
        scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Save & open app'));
    await tester.pumpAndSettle();

    expect(find.text('Public URL must be an absolute http(s) URL.'),
        findsOneWidget);
    expect(app.config, isNull);
  });

  testWidgets('AW-02c onboarding accepts the prefilled valid values',
      (tester) async {
    await setUpPumpEnv(tester);
    final core = FakeCoreRuntime();
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: null, core: core);
    await pumpScreen(tester,
        OnboardingScreen(appState: app, mode: OnboardingMode.createNew));
    await tester.pumpAndSettle();

    await tester.scrollUntilVisible(find.text('Save & open app'), 200,
        scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Save & open app'));
    await tester.pumpAndSettle();

    expect(app.config, isNotNull);
    expect(app.config!.username, 'alice');
    expect(app.config!.domain, 'relay.fedi3.com');
    expect(core.startCalls, 1);
  });

  testWidgets('AW-02d login rapido deriva la config da URL e token',
      (tester) async {
    await setUpPumpEnv(tester);
    final core = FakeCoreRuntime();
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: null, core: core);
    await pumpScreen(tester,
        OnboardingScreen(appState: app, mode: OnboardingMode.loginExisting));
    await tester.pumpAndSettle();

    final username = find.widgetWithText(TextField, 'Username');
    await tester.enterText(username, 'bob');
    final url = find.widgetWithText(TextField, 'Instance URL (https://...)');
    await tester.enterText(url, 'https://fedi3.test');
    final token =
        find.widgetWithText(TextField, 'Recovery code (relay token)');
    await tester.enterText(token, 'abcdefghijklmnop1234');
    await tester.pump();

    await tester.scrollUntilVisible(find.text('Connect'), 200,
        scrollable: find.byType(Scrollable).first);
    await tester.tap(find.text('Connect'));
    await tester.pumpAndSettle();

    expect(app.config, isNotNull);
    expect(app.config!.username, 'bob');
    expect(app.config!.domain, 'fedi3.test');
    expect(app.config!.publicBaseUrl, 'https://fedi3.test');
    expect(app.config!.relayWs, 'wss://fedi3.test');
    expect(app.config!.relayToken, 'abcdefghijklmnop1234');
    expect(core.startCalls, 1);
  });
}
