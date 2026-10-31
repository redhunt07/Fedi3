/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/config_store.dart';
import 'package:fedi3/ui/screens/edit_config_screen.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  testWidgets(
      'AW-10 saving the account screen restarts the core (replaceConfig)',
      (tester) async {
    await setUpPumpEnv(tester);
    final core = FakeCoreRuntime();
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig(), core: core);
    await pumpScreen(tester, EditConfigScreen(appState: app));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();

    // P0-12: stop + save + start, so the app is not left with a dead core.
    expect(core.startCalls, 1);
    expect(app.isRunning, isTrue);
    final saved = ConfigStore.load().data!;
    expect(saved['username'], 'alice');
    expect(saved['bind'], '127.0.0.1:8788');
  });

  testWidgets('AW-13 account screen does not overflow at 1.5x text scale',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')));
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig());
    await pumpScreen(
      tester,
      EditConfigScreen(appState: app),
      textScale: 1.5,
    );
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);
  });
}
