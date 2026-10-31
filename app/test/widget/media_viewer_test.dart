/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/media_viewer_screen.dart';
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
  testWidgets('AW-12 media download on mobile does not throw getSaveLocation',
      (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: 'bytes'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(
      tester,
      MediaViewerScreen(
        appState: app,
        url: 'http://127.0.0.1:8788/media/photo.jpg',
        mediaType: 'image/jpeg',
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Download'));
    await tester.pumpAndSettle();

    // M-31: getSaveLocation throws UnimplementedError on mobile; it must be
    // handled inside the try block, never escaping as an uncaught exception.
    expect(tester.takeException(), isNull);
  });
}
