/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/config_store.dart';
import 'package:fedi3/ui/screens/networking_settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

Future<void> _scrollToAnchor(WidgetTester tester) async {
  await tester.scrollUntilVisible(
    find.byKey(const Key('anchorActorUrlField')),
    300,
    scrollable: find.byType(Scrollable).first,
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('AW-14 anchor field loads the configured identity anchor',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(
        config: buildTestConfig(anchorActorUrl: 'https://alice.example'));
    await pumpScreen(tester, NetworkingSettingsScreen(appState: app));
    await tester.pumpAndSettle();
    await _scrollToAnchor(tester);

    final field =
        tester.widget<TextField>(find.byKey(const Key('anchorActorUrlField')));
    expect(field.controller!.text, 'https://alice.example');
    expect(find.text('Identity anchor URL'), findsOneWidget);
  });

  testWidgets('AW-15 invalid anchor URL shows an error and is not saved',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);

    final core = FakeCoreRuntime();
    final app = buildTestAppState(config: buildTestConfig(), core: core);
    await pumpScreen(tester, NetworkingSettingsScreen(appState: app));
    await tester.pumpAndSettle();
    await _scrollToAnchor(tester);

    await tester.enterText(
        find.byKey(const Key('anchorActorUrlField')), 'not a url');
    await tester.tap(find.byKey(const Key('saveAnchorButton')));
    await tester.pumpAndSettle();

    expect(
      find.text(
          'Invalid URL: use http:// or https:// without a trailing slash.'),
      findsOneWidget,
    );
    expect(app.config!.anchorActorUrl, isNull);
    expect(ConfigStore.load().data, isNull);
    expect(core.startCalls, 0);
  });

  testWidgets('AW-16 valid anchor URL is normalized and saved', (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);

    final core = FakeCoreRuntime();
    final app = buildTestAppState(config: buildTestConfig(), core: core);
    await pumpScreen(tester, NetworkingSettingsScreen(appState: app));
    await tester.pumpAndSettle();
    await _scrollToAnchor(tester);

    await tester.enterText(find.byKey(const Key('anchorActorUrlField')),
        ' https://alice.example/ ');
    await tester.tap(find.byKey(const Key('saveAnchorButton')));
    await tester.pumpAndSettle();

    expect(app.config!.anchorActorUrl, 'https://alice.example');
    final saved = ConfigStore.load().data!;
    expect(saved['anchorActorUrl'], 'https://alice.example');
    // Same update path as the other config fields: the core is restarted with
    // the new anchor in the start JSON.
    expect(core.startCalls, 1);
    expect(core.lastConfigJson, contains('"anchor_actor_url"'));
    expect(core.lastConfigJson, contains('https://alice.example'));
    expect(find.text('OK'), findsOneWidget);
  });
}
