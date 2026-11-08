/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/profile_screen.dart';
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

Map<String, dynamic> _actorDoc() => {
      'type': 'Person',
      'id': 'https://fedi3.test/users/alice',
      'preferredUsername': 'alice',
      'name': 'Alice',
      'summary': '',
      'icon': {'type': 'Image', 'url': ''},
      'image': {'type': 'Image', 'url': ''},
      'inbox': 'https://fedi3.test/users/alice/inbox',
      'outbox': 'https://fedi3.test/users/alice/outbox',
      'followers': '',
      'following': '',
      'url': 'https://fedi3.test/users/alice',
    };

void main() {
  testWidgets('profile header shows the full fediverse handle', (tester) async {
    await setUpPumpEnv(tester);
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
    client.json('GET', '/users/alice', _actorDoc());
    client.json('GET', '/_fedi3/social/status', {'status': 'accepted'});
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
      ProfileScreen(appState: app, actorUrl: 'https://fedi3.test/users/alice'),
    );
    await tester.pumpAndSettle();

    // The handle carries the host, so the raw actor URL is not duplicated.
    expect(find.text('@alice@fedi3.test'), findsOneWidget);
    expect(find.text('https://fedi3.test/users/alice'), findsNothing);
    expect(find.text('Alice'), findsWidgets);
  });
}
