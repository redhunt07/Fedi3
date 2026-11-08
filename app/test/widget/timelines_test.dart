/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/timelines_screen.dart';
import 'package:fedi3/ui/widgets/network_error_card.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

const _timelinePaths = [
  '/_fedi3/timeline/unified',
  '/_fedi3/timeline/home',
  '/_fedi3/timeline/local',
  '/_fedi3/timeline/social',
  '/_fedi3/timeline/federated',
];

Future<AppState> _runningApp() async {
  final app =
      buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
  await app.startCore();
  return app;
}

void _routeTimelines(FakeHttpClient client,
    {required int statusCode, required String body}) {
  for (final path in _timelinePaths) {
    client.route(
        'GET', path, FakeHttpResponse(statusCode: statusCode, body: body));
  }
  client.route(
    'GET',
    '/_fedi3/stream',
    FakeHttpResponse(
      headers: const {'content-type': 'text/event-stream'},
      body: '',
    ),
  );
}

void main() {
  testWidgets('an initially empty timeline still polls for new posts',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient();
    _routeTimelines(client, statusCode: 200, body: '{"items":[]}');
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app = await _runningApp();
    await pumpScreen(tester, TimelinesScreen(appState: app));
    await tester.pumpAndSettle();
    final before = client.requestsFor('GET', '/_fedi3/timeline/unified').length;
    await tester.pump(const Duration(seconds: 20));
    await tester.pumpAndSettle();
    expect(client.requestsFor('GET', '/_fedi3/timeline/unified').length,
        greaterThan(before));
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('AW-03 timeline shows the loading skeleton, not an error',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient()
      ..delay = const Duration(milliseconds: 200)
      ..route(
        'GET',
        '/_fedi3/stream',
        FakeHttpResponse(
            headers: const {'content-type': 'text/event-stream'}, body: ''),
      );
    _routeTimelines(client, statusCode: 200, body: '{"items":[],"next":null}');
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, TimelinesScreen(appState: app));
    await tester.pump();

    // While the request is in flight the screen shows a skeleton, never an
    // error card nor the empty state.
    expect(find.byType(NetworkErrorCard), findsNothing);
    expect(find.text('No items'), findsNothing);

    await tester.pumpAndSettle();
    expect(find.text('No items'), findsWidgets);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('AW-04 timeline shows NetworkErrorCard with retry on failure',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient();
    _routeTimelines(client, statusCode: 500, body: 'boom');
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, TimelinesScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.byType(NetworkErrorCard), findsWidgets);
    expect(find.text('Retry'), findsWidgets);

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('AW-05 timeline shows the empty state', (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient();
    _routeTimelines(client, statusCode: 200, body: '{"items":[],"next":null}');
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, TimelinesScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('No items'), findsWidgets);
    expect(find.byType(NetworkErrorCard), findsNothing);

    await tester.pumpWidget(const SizedBox());
  });
}
