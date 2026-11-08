/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/ui/screens/notifications_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;

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

Map<String, dynamic> _follow(String actor) => {
      'ts': 1760000000000,
      'activity': {'type': 'Follow', 'actor': actor},
    };

Map<String, dynamic> _like(String actor, String noteId) => {
      'ts': 1760000000000,
      'activity': {
        'type': 'Like',
        'actor': actor,
        'object': {'id': noteId, 'content': 'post $noteId'},
      },
    };

void main() {
  testWidgets(
      'hidden notifications recover missed events without marking them read',
      (tester) async {
    await setUpPumpEnv(tester);
    var items = [_follow('https://poll.test/users/first')];
    final client = FakeHttpClient();
    client.handler = (request) => FakeHttpResponse(
        body: request.url.path == '/_fedi3/notifications'
            ? jsonEncode({'items': items})
            : (request.url.path == '/_fedi3/stream' ? '' : '{}'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app = buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime(),
        prefs: UiPrefs.defaults().copyWith(lastNotificationsSeenMs: 1760000000000));
    await app.startCore();
    final seen = app.prefs.lastNotificationsSeenMs;
    await pumpScreen(tester, NotificationsScreen(appState: app, active: false));
    await tester.pumpAndSettle();
    expect(app.prefs.lastNotificationsSeenMs, seen);
    items = [{..._follow('https://poll.test/users/second'), 'ts': 1760000000001}, ...items];
    await tester.pump(const Duration(seconds: 15));
    await tester.pumpAndSettle();
    expect(find.text('second'), findsOneWidget);
    expect(app.unreadNotifications, 1);
    expect(app.prefs.lastNotificationsSeenMs, seen);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
      'AW-11 new notification is merged at the head without clearing the list',
      (tester) async {
    await setUpPumpEnv(tester);
    var notificationCalls = 0;
    final client = FakeHttpClient();
    client.handler = (http.BaseRequest request) async {
      final path = request.url.path;
      if (path == '/_fedi3/notifications') {
        notificationCalls++;
        final items = notificationCalls == 1
            ? [_follow('https://a.test/users/one')]
            : [
                _follow('https://a.test/users/one'),
                _follow('https://b.test/users/two')
              ];
        return FakeHttpResponse(
            body: jsonEncode({'items': items, 'next': null}));
      }
      if (path == '/_fedi3/stream') {
        return FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'},
          body: 'data: {"kind":"notification","ts_ms":1}\n\n',
        );
      }
      return FakeHttpResponse(body: '{}');
    };
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, NotificationsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('one'), findsOneWidget);
    // The SSE refresh is debounced (350 ms) and timers alone do not schedule
    // frames, so advance the clock until the stream frame arrives and the
    // debounce fires.
    for (var i = 0; i < 6 && notificationCalls < 2; i++) {
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();
    }
    expect(notificationCalls, greaterThanOrEqualTo(2));
    expect(find.text('two'), findsOneWidget);

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('AW-11b notification list keeps its items after a reload',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient();
    client.json('GET', '/_fedi3/notifications', {
      'items': [_follow('https://a.test/users/one')],
      'next': 'cursor-1',
    });
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, NotificationsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('one'), findsOneWidget);
    // Cursor is preserved: the load-more button remains available.
    expect(find.text('Load more'), findsOneWidget);

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('AW-11d notifications from one actor are grouped and expandable',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient();
    client.json('GET', '/_fedi3/notifications', {
      'items': [
        _like('https://a.test/users/one', 'n1'),
        _like('https://a.test/users/one', 'n2'),
      ],
      'next': null,
    });
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'}, body: ''),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, NotificationsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('×2'), findsOneWidget);
    expect(find.text('post n1'), findsNothing);
    await tester.tap(find.text('×2'));
    await tester.pumpAndSettle();
    expect(find.text('post n1'), findsOneWidget);
    expect(find.text('post n2'), findsOneWidget);

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('AW-11c malformed SSE frame does not break notifications',
      (tester) async {
    await setUpPumpEnv(tester);
    final client = FakeHttpClient();
    client.json(
        'GET', '/_fedi3/notifications', {'items': <dynamic>[], 'next': null});
    client.route(
      'GET',
      '/_fedi3/stream',
      FakeHttpResponse(
        headers: const {'content-type': 'text/event-stream'},
        body: 'data: {broken\n\ndata: {"kind":"notification","ts_ms":2}\n\n',
      ),
    );
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);

    final app = await _runningApp();
    await pumpScreen(tester, NotificationsScreen(appState: app));
    await tester.pumpAndSettle();

    expect(find.text('No notifications'), findsOneWidget);
    expect(tester.takeException(), isNull);

    await tester.pumpWidget(const SizedBox());
  });
}
