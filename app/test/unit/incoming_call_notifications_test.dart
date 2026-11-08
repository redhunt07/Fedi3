import 'dart:convert';
import 'package:fedi3/core/core_api.dart';
import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/services/incoming_call_notifications.dart';
import 'package:flutter_test/flutter_test.dart';
import '../support/fake_http.dart';
import '../support/test_config.dart';

void main() {
  test(
      'background polling recovers invites, deduplicates and cancels ended calls',
      () async {
    final client = FakeHttpClient();
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    final now = DateTime.now().millisecondsSinceEpoch;
    var action = 'invite';
    var id = 1;
    client.handler = (request) {
      final after =
          int.tryParse(request.url.queryParameters['after_id'] ?? '0') ?? 0;
      return FakeHttpResponse(
          body: jsonEncode({
        'items': after >= id
            ? []
            : [
                {
                  'id': id,
                  'thread_id': 'thread',
                  'call_id': 'background-call',
                  'from_actor': 'https://remote.test/users/bob',
                  'action': action,
                  'data': {'media': 'audio'},
                  'created_at_ms': now,
                }
              ]
      }));
    };
    final shown = <String>[];
    var cancelled = 0;
    final calls = IncomingCallNotifications(
        api: CoreApi(config: buildTestConfig()),
        selfActor: 'https://example.test/users/alice',
        onInvite: (s) async => shown.add(s.callId),
        onEnded: () async {
          cancelled++;
        });
    await calls.poll();
    await calls.poll();
    expect(shown, ['background-call']);
    action = 'leave';
    id = 2;
    await calls.poll();
    expect(cancelled, 1);
    expect(client.lastRequest.url.queryParameters['after_id'], '1');
  });

  test('foreground observers claim the same invite only once', () {
    expect(IncomingCallGate.claim('one-dialog'), isTrue);
    expect(IncomingCallGate.claim('one-dialog'), isFalse);
  });
}
