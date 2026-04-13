import 'package:flutter_test/flutter_test.dart';

import 'package:fedi3/model/core_config.dart';
import 'package:fedi3/model/ui_prefs.dart';

void main() {
  test('core config sanitized json excludes sensitive fields', () {
    final cfg = CoreConfig(
      username: 'alice',
      domain: 'example.com',
      publicBaseUrl: 'https://relay.example.com',
      relayWs: 'wss://relay.example.com',
      relayToken: 'relay-token-secret',
      bind: '127.0.0.1:8788',
      internalToken: 'internal-secret',
      apRelays: const ['https://relay.example.com/actor'],
      bootstrapFollowActors: const [],
      displayName: 'Alice',
      summary: 'Hello',
      iconUrl: '',
      iconMediaType: '',
      imageUrl: '',
      imageMediaType: '',
      profileFields: const [],
      manuallyApprovesFollowers: false,
      blockedDomains: const [],
      blockedActors: const [],
      previousPublicBaseUrl: 'https://old.example.com',
      previousRelayToken: 'previous-secret',
    );

    final json = cfg.toSanitizedJson();
    expect(json.containsKey('relayToken'), isFalse);
    expect(json.containsKey('internalToken'), isFalse);
    expect(json.containsKey('previousRelayToken'), isFalse);
  });

  test('ui prefs sanitized json excludes secret fields', () {
    final prefs = UiPrefs.defaults().copyWith(
      translationAuthKey: 'deepl-secret',
      relayAdminToken: 'admin-secret',
    );

    final json = prefs.toSanitizedJson();
    expect(json.containsKey('translationAuthKey'), isFalse);
    expect(json.containsKey('relayAdminToken'), isFalse);
  });
}
