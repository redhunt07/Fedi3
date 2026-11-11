/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/follow_requests_screen.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

const _alice = 'http://remote.example/users/alice';
const _bob = 'http://remote.example/users/bob';

FakeHttpClient _client({List<Map<String, dynamic>>? requests}) {
  final client = FakeHttpClient();
  final pending = requests ??
      [
        {'actor': _alice, 'created_at_ms': 1760000000000, 'state': 'pending'},
        {'actor': _bob, 'created_at_ms': 1760000001000, 'state': 'pending'},
      ];
  client.handler = (req) async {
    final path = req.url.path;
    if (req.method == 'GET' && path == '/_fedi3/follow_requests') {
      return FakeHttpResponse(body: jsonEncode(pending));
    }
    if (req.method == 'POST' && path.endsWith('/approve')) {
      return FakeHttpResponse(body: jsonEncode({'ok': true}));
    }
    if (req.method == 'POST' && path.endsWith('/reject')) {
      return FakeHttpResponse(body: jsonEncode({'ok': true}));
    }
    return FakeHttpResponse(body: '{}');
  };
  return client;
}

void main() {
  testWidgets('AW-20 follow requests list renders pending actors',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = _client();
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig());
    await app.startCore();
    await pumpScreen(tester, FollowRequestsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('alice@remote.example'), findsOneWidget);
    expect(find.text('bob@remote.example'), findsOneWidget);
    expect(client.requests.where((r) => r.method == 'GET').length, 1);
  });

  testWidgets('AW-21 approving a request calls approve and removes the row',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = _client();
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig());
    await app.startCore();
    await pumpScreen(tester, FollowRequestsScreen(appState: app));
    await tester.pumpAndSettle();

    await tester.tap(find.byIcon(Icons.check_circle_outline).first);
    await tester.pumpAndSettle();

    expect(find.text('alice@remote.example'), findsNothing);
    expect(find.text('bob@remote.example'), findsOneWidget);
    final posts = client.requests.where((r) => r.method == 'POST').toList();
    expect(posts, hasLength(1));
    expect(posts.first.url.path, contains('/approve'));
    expect(posts.first.url.path, contains(Uri.encodeComponent(_alice)));
  });

  testWidgets('AW-22 rejecting a request calls reject and removes the row',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = _client();
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig());
    await app.startCore();
    await pumpScreen(tester, FollowRequestsScreen(appState: app));
    await tester.pumpAndSettle();

    await tester.tap(find.byIcon(Icons.cancel_outlined).first);
    await tester.pumpAndSettle();

    expect(find.text('alice@remote.example'), findsNothing);
    final posts = client.requests.where((r) => r.method == 'POST').toList();
    expect(posts, hasLength(1));
    expect(posts.first.url.path, contains('/reject'));
  });

  testWidgets('AW-23 empty list shows the empty state', (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(_client(requests: []));
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig());
    await app.startCore();
    await pumpScreen(tester, FollowRequestsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('No pending requests.'), findsOneWidget);
  });

  testWidgets('AW-24 stopped core shows the start-core hint', (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(_client());
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig());
    await pumpScreen(tester, FollowRequestsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('Start the core to see follow requests.'), findsOneWidget);
  });

  testWidgets('AW-25 a failing request shows the retry card', (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient(
      defaultResponse: FakeHttpResponse(statusCode: 500, body: 'boom'),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = buildTestAppState(config: buildTestConfig());
    await app.startCore();
    await pumpScreen(tester, FollowRequestsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(
        find.textContaining('Could not load follow requests'), findsOneWidget);
  });
}
