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
