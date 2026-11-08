/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/relays_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

FakeHttpClient _client() {
  final client =
      FakeHttpClient(defaultResponse: FakeHttpResponse(statusCode: 404));
  client.json('GET', '/_fedi3/relay/relays', {'relays': []});
  client.json('GET', '/_fedi3/relay/stats', {
    'relay_url': 'https://relay.example',
    'timestamp_ms': 1794100000000,
    'online_users': 2,
    'online_peers': 3,
    'total_users': 4,
    'total_peers_seen': 5,
    'relays': ['https://relay.example', 'https://other.example'],
    'search_indexed_users': 4,
    'search_total_users': 4,
    'search_last_index_ms': 1794100000000,
  });
  client.json('GET', '/_fedi3/relay/search/coverage', {
    'total_users': 4,
    'indexed_users': 4,
    'last_index_ms': 1794100000000,
  });
  client.json('GET', '/_fedi3/relay/peers', {'items': []});
  return client;
}

void main() {
  testWidgets('telemetry renders as labelled rows, not a raw dump',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(_client());
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: buildTestConfig());

    await pumpScreen(tester, RelaysScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('Telemetry'), findsOneWidget);
    expect(find.text('Online users'), findsOneWidget);
    expect(find.text('2'), findsWidgets);
    expect(find.text('Online peers'), findsOneWidget);
    expect(find.text('3'), findsWidgets);
    expect(find.text('Known users'), findsOneWidget);
    expect(find.text('Peers seen'), findsOneWidget);
    expect(find.text('Relays in mesh'), findsOneWidget);
    // The raw JSON must be collapsed by default.
    expect(find.byType(SelectableText), findsNothing);
  });

  testWidgets('raw telemetry panel toggles from the card action',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(_client());
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: buildTestConfig());

    await pumpScreen(tester, RelaysScreen(appState: app));
    await tester.pumpAndSettle();

    final action = find.text('View raw telemetry');
    expect(action, findsOneWidget);
    await tester.tap(action);
    await tester.pumpAndSettle();

    final raw = find.byType(SelectableText);
    expect(raw, findsOneWidget);
    expect(
      tester.widget<SelectableText>(raw).data,
      contains('"online_users": 2'),
    );

    await tester.tap(action);
    await tester.pumpAndSettle();
    expect(find.byType(SelectableText), findsNothing);
  });

  testWidgets('relay tiles show relative last-seen and latency',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = _client();
    final now = DateTime.now().millisecondsSinceEpoch;
    client.json('GET', '/_fedi3/relay/relays', {
      'relays': [
        {
          'relay_url': 'https://relay.example',
          'last_seen_ms': now - 120000,
          'telemetry': {'search_indexed_users': 4, 'search_total_users': 4},
        }
      ],
    });
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: buildTestConfig());

    await pumpScreen(tester, RelaysScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.textContaining('Last seen'), findsWidgets);
    expect(find.textContaining('last_seen='), findsNothing);
  });
}
