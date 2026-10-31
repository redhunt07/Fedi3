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
