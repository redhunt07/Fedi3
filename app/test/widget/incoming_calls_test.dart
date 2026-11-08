import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  testWidgets('a call in another chat rings while a pushed screen is open',
      (tester) async {
    await setUpPumpEnv(tester);
    var ready = false;
    final now = DateTime.now().millisecondsSinceEpoch;
    final client = FakeHttpClient();
    client.handler = (request) => FakeHttpResponse(
            body: jsonEncode({
          'items': ready && request.url.path == '/_fedi3/chat/call/signals'
              ? [
                  {
                    'id': 99,
                    'thread_id': 'other-chat',
                    'thread_title': 'Other caller',
                    'from_actor': 'https://remote.test/users/bob',
                    'call_id': 'while-settings',
                    'action': 'invite',
                    'data': {'media': 'audio'},
                    'created_at_ms': now,
                  }
                ]
              : []
        }));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app =
        buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
    await app.startCore();
    await pumpScreen(tester, Shell(appState: app));
    await tester.pumpAndSettle();
    Navigator.of(tester.element(find.byType(Shell))).push(
        MaterialPageRoute<void>(
            builder: (_) => const Scaffold(body: Text('Another screen'))));
    await tester.pumpAndSettle();
    ready = true;
    // The shell polls every 3 s. Pump past a couple of cadences so the invite
    // is fetched after [ready] flips, independent of when the previous timer
    // tick happened during the route transition.
    for (var i = 0; i < 12; i++) {
      await tester.pump(const Duration(seconds: 1));
      if (find.byType(AlertDialog).evaluate().isNotEmpty) break;
    }
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.text('Other caller'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('global incoming call polling makes one request across chats',
      (tester) async {
    await setUpPumpEnv(tester);
    final now = DateTime.now().millisecondsSinceEpoch;
    final client = FakeHttpClient();
    client.handler = (request) {
      if (request.url.path == '/_fedi3/chat/threads') {
        return FakeHttpResponse(
            body: jsonEncode({
          'items': [
            {'thread_id': 'newer-chat', 'title': 'Newer'},
            {'thread_id': 'incoming-chat', 'title': 'Incoming'},
          ],
        }));
      }
      if (request.url.path == '/_fedi3/chat/call/signals') {
        expect(request.url.queryParameters.containsKey('thread_id'), isFalse);
        final timestamp = now;
        final after = int.parse(request.url.queryParameters['after_ms']!);
        return FakeHttpResponse(
            body: jsonEncode({
          'items': timestamp <= after
              ? []
              : [
                  {
                    'id': 2,
                    'thread_id': 'incoming-chat',
                    'thread_title': 'Incoming',
                    'from_actor': 'https://remote.test/users/bob',
                    'call_id': 'incoming-call',
                    'action': 'invite',
                    'data': {'media': 'video'},
                    'created_at_ms': timestamp,
                  },
                ],
        }));
      }
      return FakeHttpResponse(body: '{"items":[],"next":null}');
    };
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app =
        buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
    await app.startCore();
    await pumpScreen(tester, Shell(appState: app));
    for (var i = 0; i < 8; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(tester.takeException(), isNull);
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.text('Incoming'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });
}
