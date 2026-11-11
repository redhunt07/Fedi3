import 'dart:convert';
import 'dart:async';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/call_screen.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  testWidgets(
      'closing while microphone capture starts releases late native resources',
      (tester) async {
    await setUpPumpEnv(tester);
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final captured = Completer<Map<String, dynamic>>();
    final methods = <String>[];
    for (final name in ['FlutterWebRTC.Event', 'FlutterWebRTC/Texture1']) {
      messenger.setMockMethodCallHandler(
          MethodChannel(name), (_) async => null);
    }
    messenger.setMockMethodCallHandler(
        const MethodChannel('FlutterWebRTC.Method'), (call) async {
      methods.add(call.method);
      if (call.method == 'createVideoRenderer') return {'textureId': 1};
      if (call.method == 'getUserMedia') return captured.future;
      return null;
    });
    addTearDown(() => messenger.setMockMethodCallHandler(
        const MethodChannel('FlutterWebRTC.Method'), null));
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{"items":[]}'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    await pumpScreen(
        tester,
        CallScreen(
            appState: buildTestAppState(config: buildTestConfig()),
            threadId: 't1',
            title: 'Call',
            mediaType: 'audio'));
    await tester.pump();
    expect(methods, contains('getUserMedia'));
    await tester.pumpWidget(const SizedBox());
    captured.complete({
      'streamId': 'late-stream',
      'audioTracks': [
        {
          'id': 'late-mic',
          'label': 'Microphone',
          'kind': 'audio',
          'enabled': true
        }
      ],
      'videoTracks': []
    });
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(methods, contains('trackDispose'));
    expect(methods, contains('streamDispose'));
    expect(client.requestsFor('POST', '/_fedi3/chat/call/signal'), isEmpty);
  });
  testWidgets(
      'accepted direct call creates offer and keeps early ICE until answer',
      (tester) async {
    await setUpPumpEnv(tester);
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final nativeCalls = <String>[];
    for (final channel in [
      'FlutterWebRTC.Event',
      'FlutterWebRTC/Texture1',
      'FlutterWebRTC/peerConnectionEventpc-test',
    ]) {
      messenger.setMockMethodCallHandler(
          MethodChannel(channel), (_) async => null);
    }
    messenger.setMockMethodCallHandler(
        const MethodChannel('FlutterWebRTC.Method'), (call) async {
      nativeCalls.add(call.method);
      switch (call.method) {
        case 'createVideoRenderer':
          return {'textureId': 1};
        case 'getUserMedia':
          return {
            'streamId': 'local-test',
            'audioTracks': [],
            'videoTracks': []
          };
        case 'createPeerConnection':
          return {'peerConnectionId': 'pc-test'};
        case 'createOffer':
          return {'sdp': 'test-offer', 'type': 'offer'};
        case 'getRemoteDescription':
          return {'sdp': 'test-answer', 'type': 'answer'};
        default:
          return null;
      }
    });
    final client = FakeHttpClient();
    String? callId;
    final sentActions = <String>[];
    int? firstSignalMs;
    client.handler = (request) {
      if (request.url.path == '/_fedi3/chat/call/signal') {
        final body = jsonDecode(client.lastBody) as Map<String, dynamic>;
        callId = body['call_id'] as String;
        sentActions.add(body['action'] as String);
        return FakeHttpResponse(body: '{"ok":true,"sent":1,"queued":0}');
      }
      if (request.url.path == '/_fedi3/chat/call/signals' && callId != null) {
        final after = int.parse(request.url.queryParameters['after_ms']!);
        firstSignalMs ??= after + 1;
        final now = firstSignalMs!;
        final rows = [
          {
            'id': 1,
            'action': 'ice',
            'data': {
              'candidate': 'early-candidate',
              'sdp_mid': '0',
              'sdp_mline_index': 0
            },
            'created_at_ms': now
          },
          {'id': 2, 'action': 'accept', 'data': {}, 'created_at_ms': now + 1},
          {
            'id': 3,
            'action': 'answer',
            'data': {'sdp': 'test-answer', 'type': 'answer'},
            'created_at_ms': now + 2
          },
        ]
            .where((s) => (s['created_at_ms'] as int) > after)
            .map((s) => {
                  ...s,
                  'call_id': callId,
                  'from_actor': 'https://remote.test/users/bob',
                })
            .toList();
        return FakeHttpResponse(body: jsonEncode({'items': rows}));
      }
      return FakeHttpResponse(body: '{"items":[]}');
    };
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    await pumpScreen(
        tester,
        CallScreen(
          appState: buildTestAppState(config: buildTestConfig()),
          threadId: 'thread-test',
          title: 'Bob',
          mediaType: 'audio',
        ));
    // The caller waits briefly for ICE gathering so the offer carries
    // candidates; the fake clock must advance past that window.
    for (var i = 0; i < 40; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(tester.takeException(), isNull);
    expect(sentActions.where((s) => s == 'offer'), hasLength(1),
        reason:
            'native=$nativeCalls; sent=$sentActions; requests=${client.requests.map((r) => r.url.path).toList()}; texts=${tester.widgetList<Text>(find.byType(Text)).map((w) => w.data).toList()}');
    expect(nativeCalls, contains('createPeerConnection'));
    expect(nativeCalls, contains('addCandidate'));
    expect(nativeCalls.indexOf('setRemoteDescription'),
        lessThan(nativeCalls.indexOf('addCandidate')));
    // Receiving an SDP answer alone must not claim a connected media session.
    expect(find.text('Connecting…'), findsWidgets);
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
  });
}
