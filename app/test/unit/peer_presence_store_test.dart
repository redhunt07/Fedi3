import 'dart:async';

import 'package:fedi3/core/core_api.dart';
import 'package:fedi3/state/peer_presence_store.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/test_config.dart';

void main() {
  test(
      'equal usernames across domains never share presence and stale devices cannot overwrite an online device',
      () async {
    final client = FakeHttpClient();
    client.json('GET', '/_fedi3/relay/peers', {
      'items': [
        {
          'username': 'panormus',
          'actor_url': 'https://fedi.example/users/panormus',
          'online': true
        },
        {
          'username': 'panormus',
          'actor_url': 'https://fedi.example/users/panormus',
          'online': false
        },
      ]
    });
    client.json('POST', '/_fedi3/presence', {});
    client.json('GET', '/_fedi3/presence', {});
    final store = PeerPresenceStore.forTesting(
        (config) => CoreApi(config: config, client: client));
    addTearDown(store.stop);
    final updated = Completer<void>();
    store.statusByActor.addListener(() {
      if (!updated.isCompleted) updated.complete();
    });
    store.start(buildTestConfig());
    await updated.future.timeout(const Duration(seconds: 2));
    expect(store.statusFor('https://fedi.example/users/panormus'),
        PresenceStatus.online);
    expect(store.statusFor('https://sharkey.example/users/panormus'),
        PresenceStatus.offline);
    expect(store.statusFor('panormus'), PresenceStatus.offline);
  });

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
    store.statusByActor.addListener(() {
      if (!updated.isCompleted) updated.complete();
    });
    store.start(buildTestConfig());
    await updated.future.timeout(const Duration(seconds: 2));
    expect(
        store.statusFor('https://peer.example/users/Bob'), PresenceStatus.busy);
  });

  test('relay presence survives an unavailable local presence endpoint',
      () async {
    final client = FakeHttpClient();
    client.json('GET', '/_fedi3/relay/peers', {
      'items': [
        {
          'username': 'Bob',
          'actor_url': 'https://relay.example/users/Bob',
          'online': true
        }
      ],
    });
    client.json('POST', '/_fedi3/presence', {});
    client.json('GET', '/_fedi3/presence', {}, statusCode: 503);
    final store = PeerPresenceStore.forTesting(
      (config) => CoreApi(config: config, client: client),
    );
    addTearDown(store.stop);
    final updated = Completer<void>();
    store.statusByActor.addListener(() {
      if (!updated.isCompleted) updated.complete();
    });
    store.start(buildTestConfig());
    await updated.future.timeout(const Duration(seconds: 2));
    expect(store.statusFor('https://relay.example/users/Bob'),
        PresenceStatus.online);
  });
}
