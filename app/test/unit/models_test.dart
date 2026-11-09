/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:fedi3/model/core_config.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/services/backup_codec.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/test_config.dart';

void main() {
  group('CoreConfig serialization (AM-01..04)', () {
    test('peer discovery options survive edits and reach the Rust core', () {
      final cfg = buildTestConfig().copyWith(
        p2pBootstrap: ['/dns4/seed.example/tcp/4001/p2p/test'],
        p2pPublicDiscovery: false,
        p2pPeerExchange: false,
      );
      final decoded =
          CoreConfig.fromJson(cfg.toJson()).copyWith(displayName: 'Alice');
      expect(decoded.p2pBootstrap, cfg.p2pBootstrap);
      expect(decoded.p2pPublicDiscovery, isFalse);
      expect(decoded.p2pPeerExchange, isFalse);
      final p2p = decoded.toCoreStartJson()['p2p'] as Map;
      expect(p2p['bootstrap'], cfg.p2pBootstrap);
      expect(p2p['public_discovery_enable'], isFalse);
      expect(p2p['peer_exchange_enable'], isFalse);
      expect(CoreConfig.fromJson({}).p2pPublicDiscovery, isTrue);
      expect(CoreConfig.fromJson({}).p2pPeerExchange, isTrue);
    });
    test('AM-01 fromJson(toJson()) round-trips identically', () {
      final cfg = buildTestConfig(
        p2pEnable: true,
        relayToken: 'token-abcdefghijklmnop',
        internalToken: 'internal-abcdefghijklmnop',
      );
      final json = cfg.toJson();
      final decoded = CoreConfig.fromJson(
          jsonDecode(jsonEncode(json)) as Map<String, dynamic>);
      expect(decoded.toJson(), equals(json));
    });

    test('AM-02 toCoreStartJson uses snake_case keys and includes tokens', () {
      final cfg = buildTestConfig();
      final start = cfg.toCoreStartJson();
      expect(start['username'], 'alice');
      expect(start['public_base_url'], 'https://example.test');
      expect(start['relay_ws'], 'wss://relay.example.test');
      expect(start['relay_token'], cfg.relayToken);
      expect(start['internal_token'], cfg.internalToken);
      expect(start['post_delivery_mode'], CoreConfig.postDeliveryP2pRelay);
      expect(start['p2p'], isA<Map>());
    });

    test('AM-03 missing fields fall back to safe defaults', () {
      final decoded = CoreConfig.fromJson(<String, dynamic>{});
      expect(decoded.username, '');
      expect(decoded.domain, '');
      expect(decoded.p2pEnable, isFalse);
      expect(decoded.webrtcEnable, isFalse);
      expect(decoded.apRelays, isEmpty);
      expect(decoded.postDeliveryMode, CoreConfig.postDeliveryP2pRelay);
      expect(decoded.profileFields, isEmpty);
    });

    test('AM-04 wrongly typed fields do not crash', () {
      final decoded = CoreConfig.fromJson(<String, dynamic>{
        'username': 42,
        'domain': <String>['x'],
        'p2pEnable': 'yes',
        'p2pRelayReserve': 'not-a-list',
        'profileFields': [
          1,
          {'name': 'website', 'value': 'https://x'}
        ],
        'p2pCacheTtlSecs': 'soon',
        'webrtcIceUsername': 7,
      });
      expect(decoded.username, '');
      expect(decoded.domain, '');
      expect(decoded.p2pEnable, isFalse);
      expect(decoded.p2pRelayReserve, isEmpty);
      expect(decoded.profileFields, hasLength(1));
      expect(decoded.profileFields.single.name, 'website');
      expect(decoded.p2pCacheTtlSecs, isNull);
      expect(decoded.webrtcIceUsername, isNull);
    });

    test('AM-08 anchorActorUrl round-trips and is sent to the core', () {
      final cfg = buildTestConfig(anchorActorUrl: 'https://alice.example/');
      expect(cfg.anchorActorUrl, 'https://alice.example');
      expect(cfg.toJson()['anchorActorUrl'], 'https://alice.example');
      final decoded = CoreConfig.fromJson(
          jsonDecode(jsonEncode(cfg.toJson())) as Map<String, dynamic>);
      expect(decoded.anchorActorUrl, 'https://alice.example');
      expect(decoded.toJson(), equals(cfg.toJson()));
      expect(decoded.toCoreStartJson()['anchor_actor_url'],
          'https://alice.example');

      final http = buildTestConfig(anchorActorUrl: 'http://anchor.test');
      expect(http.toCoreStartJson()['anchor_actor_url'], 'http://anchor.test');
    });

    test('AM-09 missing anchorActorUrl stays null and is not sent', () {
      final cfg = buildTestConfig();
      expect(cfg.anchorActorUrl, isNull);
      expect(cfg.toJson().containsKey('anchorActorUrl'), isTrue);
      expect(cfg.toJson()['anchorActorUrl'], isNull);
      expect(cfg.toCoreStartJson().containsKey('anchor_actor_url'), isFalse);

      final decoded = CoreConfig.fromJson(<String, dynamic>{});
      expect(decoded.anchorActorUrl, isNull);
      expect(
          decoded.toCoreStartJson().containsKey('anchor_actor_url'), isFalse);
    });

    test('AM-10 invalid anchorActorUrl values normalize to null', () {
      for (final bad in <dynamic>[
        'ftp://alice.example',
        'alice.example',
        'https://',
        '   ',
        42,
        null,
      ]) {
        final decoded = CoreConfig.fromJson(<String, dynamic>{
          'anchorActorUrl': bad,
        });
        expect(decoded.anchorActorUrl, isNull, reason: 'value: $bad');
        expect(
            decoded.toCoreStartJson().containsKey('anchor_actor_url'), isFalse,
            reason: 'value: $bad');
      }
      final normalized =
          buildTestConfig(anchorActorUrl: '  https://alice.example///  ');
      expect(normalized.anchorActorUrl, 'https://alice.example');
    });

    test('AM-11 identity and device sync fields reach the core', () {
      final cfg = buildTestConfig().copyWith(
        identityPrivateKeyPem:
            '-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----',
        chatIdentityPublicB64: 'cHVibGlj',
        chatIdentitySecretB64: 'c2VjcmV0',
        deviceSyncEnable: false,
      );
      final json = cfg.toJson();
      expect(json['identityPrivateKeyPem'], cfg.identityPrivateKeyPem);
      expect(json['chatIdentityPublicB64'], 'cHVibGlj');
      expect(json['chatIdentitySecretB64'], 'c2VjcmV0');
      expect(json['deviceSyncEnable'], isFalse);

      final decoded = CoreConfig.fromJson(
          jsonDecode(jsonEncode(json)) as Map<String, dynamic>);
      expect(decoded.toJson(), equals(json));
      expect(decoded.identityPrivateKeyPem, cfg.identityPrivateKeyPem);
      expect(decoded.deviceSyncEnable, isFalse);

      final start = decoded.toCoreStartJson();
      expect(start['identity_private_key_pem'], cfg.identityPrivateKeyPem);
      expect(start['chat_identity_public_b64'], 'cHVibGlj');
      expect(start['chat_identity_secret_b64'], 'c2VjcmV0');
      final p2p = start['p2p'] as Map;
      expect(p2p['device_sync_enable'], isFalse);
      expect(p2p['device_sync_poll_secs'], 3);

      final defaults = CoreConfig.fromJson(<String, dynamic>{});
      expect(defaults.deviceSyncEnable, isTrue);
      expect((defaults.toCoreStartJson()['p2p'] as Map)['device_sync_enable'],
          isTrue);
      expect(defaults.toCoreStartJson().containsKey('identity_private_key_pem'),
          isFalse);

      final cleared = cfg.copyWith(identityPrivateKeyPem: null);
      expect(cleared.identityPrivateKeyPem, isNull);
      expect(cleared.toCoreStartJson().containsKey('identity_private_key_pem'),
          isFalse);
    });
  });

  group('UiPrefs serialization (AM-05)', () {
    test('AM-05 UiPrefs round-trips identically', () {
      final prefs = UiPrefs.defaults().copyWith(
        themeMode: UiThemeMode.dark,
        textScale: 1.2,
        localeTag: 'it',
        mediaAutoplay: false,
        relayAdminToken: 'admin-token',
        chatThreadSeenMs: const {'t1': 123},
      );
      final decoded = UiPrefs.fromJson(
          jsonDecode(jsonEncode(prefs.toJson())) as Map<String, dynamic>);
      expect(decoded.toJson(), equals(prefs.toJson()));
    });
  });

  group('BackupCodec (AM-06..07)', () {
    test('AM-06 encode/decode round-trip', () {
      final cfg = buildTestConfig();
      final prefs = UiPrefs.defaults();
      final text = BackupCodec.encode(config: cfg, prefs: prefs);
      final bundle = BackupCodec.decode(text);
      expect(bundle.config.username, cfg.username);
      expect(bundle.config.publicBaseUrl, cfg.publicBaseUrl);
      expect(bundle.prefs.toJson(), equals(prefs.toJson()));
    });

    test('AM-06 rejects a backup written by a newer app version', () {
      final text = BackupCodec.encode(
          config: buildTestConfig(), prefs: UiPrefs.defaults());
      final json = jsonDecode(text) as Map<String, dynamic>;
      json['v'] = BackupCodec.version + 1;
      expect(
        () => BackupCodec.decode(jsonEncode(json)),
        throwsA(isA<FormatException>()),
      );
    });

    test('AM-07 corrupt JSON throws a clear FormatException', () {
      expect(() => BackupCodec.decode('{not json'),
          throwsA(isA<FormatException>()));
      expect(() => BackupCodec.decode(''), throwsA(isA<FormatException>()));
      expect(
          () => BackupCodec.decode('{"v":1}'), throwsA(isA<FormatException>()));
    });
  });
}
