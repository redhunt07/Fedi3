import 'package:fedi3/services/call_ice_config.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/test_config.dart';

void main() {
  test(
      'unconfigured calls use independent STUN services without relay credentials',
      () {
    final servers = buildCallIceServers([],
        username: 'local-user', credential: 'local-secret');
    expect(servers.map((server) => server['urls']), defaultCallIceUrls);
    expect(
        servers.every((server) => !server.containsKey('credential')), isTrue);
  });
  test('native calls receive STUN and each TURN transport separately', () {
    final servers = buildCallIceServers([
      'stun:relay.example:3478',
      'turn:relay.example:3478?transport=udp',
      'turn:relay.example:3478?transport=tcp',
    ], username: 'test-user', credential: 'test-secret');
    expect(servers, hasLength(3));
    expect(servers.map((s) => s['urls']), everyElement(isA<String>()));
    expect(servers.first.containsKey('credential'), isFalse);
    expect(servers.last['urls'], contains('transport=tcp'));
    expect(servers.last['credential'], 'test-secret');
  });

  test('operator credentials derive TURN endpoints from the relay host', () {
    final servers = buildCallIceServers(
      ['stun:relay.example:3478'],
      username: 'relay',
      credential: 'secret',
      relayHost: 'relay.example',
    );
    final urls = servers.map((server) => server['urls']).toList();
    expect(urls, contains('turn:relay.example:3478?transport=udp'));
    expect(urls, contains('turn:relay.example:3478?transport=tcp'));
    for (final server in servers) {
      if (server['urls'].toString().startsWith('turn:')) {
        expect(server['username'], 'relay');
        expect(server['credential'], 'secret');
      } else {
        expect(server.containsKey('credential'), isFalse);
      }
    }
  });

  test('an explicit TURN URL suppresses relay host derivation', () {
    final servers = buildCallIceServers(
      ['turn:turn.example:3478?transport=udp'],
      username: 'relay',
      credential: 'secret',
      relayHost: 'relay.example',
    );
    expect(servers, hasLength(1));
    expect(servers.single['urls'], 'turn:turn.example:3478?transport=udp');
  });

  test('credentials alone do not invent TURN endpoints', () {
    final servers = buildCallIceServers([],
        username: 'relay', credential: 'secret', relayHost: null);
    expect(servers.map((server) => server['urls']), defaultCallIceUrls);
    expect(
        servers.every((server) => !server.containsKey('credential')), isTrue);
  });

  test('relay host comes from the websocket URL or the public base URL', () {
    expect(callRelayHost('wss://relay.example/ws', ''), 'relay.example');
    expect(callRelayHost('', 'https://relay.example'), 'relay.example');
    expect(callRelayHost('', ''), isNull);
  });

  test('TCP and TLS TURN settings survive persistence but stay out of Rust ICE',
      () {
    final cfg = buildTestConfig().copyWith(webrtcIceUrls: [
      'stun:relay.example:3478',
      'turn:relay.example:3478?transport=udp',
      'turn:relay.example:3478?transport=tcp',
      'turns:relay.example:5349',
    ]);
    expect(cfg.toJson()['webrtcIceUrls'], hasLength(4));
    final core = cfg.toCoreStartJson();
    expect((core['p2p'] as Map)['webrtc_ice_urls'], [
      'stun:relay.example:3478',
      'turn:relay.example:3478?transport=udp',
    ]);
  });
}
