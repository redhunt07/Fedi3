/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/app_root.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_core.dart';
import 'support/fake_http.dart';
import 'support/pump_app.dart';

void main() {
  testWidgets('AW-01 first run shows the real welcome title (test repaired)',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = AppState(
      config: null,
      prefs: UiPrefs.defaults(),
      core: FakeCoreRuntime(),
      watchdogEnabled: false,
    );
    await tester.pumpWidget(AppRoot(appState: app));
    await tester.pumpAndSettle();

    // H-59: the old test asserted 'Fedi3 setup', but FirstRunScreen renders
    // `firstRunTitle` ("Welcome").
    expect(find.text('Welcome'), findsOneWidget);
    expect(find.text('Fedi3 setup'), findsNothing);
  });

  testWidgets('first run with no network does not throw (fake client)',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient(sendError: Exception('offline'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = AppState(
      config: null,
      prefs: UiPrefs.defaults(),
      core: FakeCoreRuntime(),
      watchdogEnabled: false,
    );
    await tester.pumpWidget(AppRoot(appState: app));
    await tester.pumpAndSettle();
    expect(find.text('Welcome'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
