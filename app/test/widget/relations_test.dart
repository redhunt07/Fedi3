import 'dart:async';
import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/relations_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  testWidgets('relations appear while an actor server is unresponsive',
      (tester) async {
    await setUpPumpEnv(tester);
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(320, 700);
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final blocked = Completer<FakeHttpResponse>();
    final client = FakeHttpClient();
    client.handler = (request) {
      if (request.url.path == '/followers') {
        return FakeHttpResponse(
            body: jsonEncode({
          'items': [
            'https://slow.test/users/slow',
            'https://fast.test/users/ready'
          ]
        }));
      }
      if (request.url.host == 'slow.test') return blocked.future;
      if (request.url.path == '/users/ready') {
        return FakeHttpResponse(
            body: jsonEncode({
          'type': 'Person',
          'id': 'https://fast.test/users/ready',
          'name': 'Ready immediately',
          'preferredUsername': 'ready'
        }));
      }
      return FakeHttpResponse(body: '{"status":"accepted"}');
    };
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final app =
        buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
    await app.startCore();
    await pumpScreen(
        tester,
        RelationsScreen(
            appState: app,
            actorUrl: 'https://owner.test/users/alice',
            collectionUrl: 'https://owner.test/followers',
            title: 'Followers'),
        textScale: 1.5);
    await tester.pumpAndSettle();
    expect(find.text('Ready immediately'), findsOneWidget);
    expect(find.text('@slow.test'), findsWidgets);
    expect(tester.takeException(), isNull);
    blocked.complete(FakeHttpResponse(statusCode: 503));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
  });
}
