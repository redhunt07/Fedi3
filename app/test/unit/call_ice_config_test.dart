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
