import 'dart:async';

import 'package:fedi3/core/core_api.dart';
import 'package:fedi3/state/peer_presence_store.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/test_config.dart';

void main() {
  test('P2P presence survives an unavailable relay directory', () async {
    final client = FakeHttpClient();
    client.json('GET', '/_fedi3/relay/peers', {}, statusCode: 503);
    client.json('POST', '/_fedi3/presence', {});
    client.json('GET', '/_fedi3/presence', {
      'items': [
        {
          'actor_id': 'https://peer.example/users/Bob',
          'status': 'busy',
          'updated_at_ms': DateTime.now().millisecondsSinceEpoch,
        },
      ],
    });
    final store = PeerPresenceStore.forTesting(
      (config) => CoreApi(config: config, client: client),
    );
    addTearDown(store.stop);
    final updated = Completer<void>();
    store.statusByUsername.addListener(() {
      if (!updated.isCompleted) updated.complete();
    });
    store.start(buildTestConfig());
    await updated.future.timeout(const Duration(seconds: 2));
    expect(store.statusFor('BOB'), PresenceStatus.busy);
  });

  test('relay presence survives an unavailable local presence endpoint',
      () async {
    final client = FakeHttpClient();
    client.json('GET', '/_fedi3/relay/peers', {
      'items': [
        {'username': 'Bob', 'online': true}
      ],
    });
    client.json('POST', '/_fedi3/presence', {});
    client.json('GET', '/_fedi3/presence', {}, statusCode: 503);
    final store = PeerPresenceStore.forTesting(
      (config) => CoreApi(config: config, client: client),
    );
    addTearDown(store.stop);
    final updated = Completer<void>();
    store.statusByUsername.addListener(() {
      if (!updated.isCompleted) updated.complete();
    });
    store.start(buildTestConfig());
    await updated.future.timeout(const Duration(seconds: 2));
    expect(store.statusFor('bob'), PresenceStatus.online);
  });
}
