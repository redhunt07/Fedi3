import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/model/peer_account.dart';
import '../support/fake_http.dart';
import '../support/fake_core.dart';
import '../support/test_config.dart';

void main() {
  test(
      'peers group by full actor without merging equal names on different relays',
      () {
    final groups = groupPeersByAccount([
      {
        'username': 'alice',
        'actor_url': 'https://one.example/users/alice',
        'peer_id': 'device-a',
        'online': false
      },
      {
        'username': 'alice',
        'actor_url': 'https://one.example/users/alice/',
        'peer_id': 'device-b',
        'online': true
      },
      {
        'username': 'alice',
        'actor_url': 'https://two.example/users/alice',
        'peer_id': 'device-c',
        'online': true
      },
    ]);
    expect(groups, hasLength(2));
    expect(groups.first['devices'], hasLength(2));
    expect(groups.first['online'], isTrue);
  });
  test(
      'replicated profile updates config and permits clears without replacing identity or restarting core',
      () async {
    final dir = await Directory.systemTemp.createTemp('account-profile-test-');
    useTempStores(dir);
    addTearDown(() async {
      resetStoreOverrides();
      CoreHttp.reset();
      await dir.delete(recursive: true);
    });
    final cfg = buildTestConfig().copyWith(
        displayName: 'Old',
        blockedActors: ['https://blocked.example/actor'],
        identityPrivateKeyPem: 'unchanged-private-key');
    final core = FakeCoreRuntime();
    final app = buildTestAppState(config: cfg, core: core);
    addTearDown(app.dispose);
    await app.startCore();
    final client = FakeHttpClient();
    client.json('GET', '/_fedi3/account/profile', {
      'profile': {
        'display_name': 'New',
        'summary': '',
        'icon_url': 'https://media.example/avatar.png',
        'profile_fields': [],
        'blocked_actors': [],
        'blocked_domains': [],
        'manually_approves_followers': false
      }
    });
    CoreHttp.useTestClient(client);
    await app.refreshAccountProfile();
    expect(app.config!.displayName, 'New');
    expect(app.config!.blockedActors, isEmpty);
    expect(app.config!.identityPrivateKeyPem, 'unchanged-private-key');
    expect(app.config!.relayToken, cfg.relayToken);
    await app.startCore();
    expect(core.startCalls, 1);
    expect(core.stopCalls, 0);
  });
}
