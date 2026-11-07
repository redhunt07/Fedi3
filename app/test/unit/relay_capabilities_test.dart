import 'dart:convert';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/services/relay_capabilities.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/test_config.dart';

void main() {
  tearDown(CoreHttp.reset);

  test('TURN operators keep distinct credentials for each native ICE URL', () {
    final pool = RelayCapabilities.parseTurnPool([
      {
        'urls': ['turn:a.example:3478?transport=udp', 'turns:a.example:5349'],
        'username': 'a',
        'credential': 'secret-a'
      },
      {
        'urls': ['turn:b.example:3478'],
        'username': 'b',
        'credential': 'secret-b'
      },
    ]);
    expect(pool, hasLength(3));
    expect(pool[0]['credential'], 'secret-a');
    expect(pool[1]['credential'], 'secret-a');
    expect(pool[2]['credential'], 'secret-b');
    expect(pool.every((item) => item['urls'] is String), isTrue);
    expect(
        RelayCapabilities.parseTurnPool([
          {
            'urls': ['https://bad.example']
          }
        ]),
        isEmpty);
  });

  test('provisioning never follows a redirect with relay credentials',
      () async {
    final client =
        FakeHttpClient(defaultResponse: FakeHttpResponse(statusCode: 302));
    CoreHttp.useTestClient(client);
    final cfg = buildTestConfig().copyWith(relayWs: 'wss://relay.example');
    expect(await RelayCapabilities.turnPool(cfg), isEmpty);
    expect(client.requests.single.followRedirects, isFalse);
    expect(client.requests.single.url.host, 'relay.example');
    expect(client.requests.single.headers['Authorization'],
        'Bearer ${cfg.relayToken}');
  });

  test('push registration sends only provider and opaque device token',
      () async {
    final client = FakeHttpClient(defaultResponse: FakeHttpResponse());
    CoreHttp.useTestClient(client);
    final cfg = buildTestConfig().copyWith(relayWs: 'wss://relay.example');
    await RelayCapabilities.registerPush(cfg, 'apns', 'opaque-test-token');
    final request = client.requests.single;
    expect(request.method, 'PUT');
    expect(jsonDecode((request as dynamic).body as String), {
      'provider': 'apns',
      'token': 'opaque-test-token',
    });
  });
}
