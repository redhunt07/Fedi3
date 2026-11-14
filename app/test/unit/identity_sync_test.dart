/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:cryptography/cryptography.dart';
import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/model/core_config.dart';
import 'package:fedi3/services/identity_sync.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/test_config.dart';

void main() {
  group('IdentitySync envelope (multi-device)', () {
    test('seal/open round-trip keeps the identity package intact', () async {
      final package = <String, dynamic>{
        'identity_private_key_pem':
            '-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----',
        'chat_identity_public_b64': 'cHVibGlj',
        'chat_identity_secret_b64': 'c2VjcmV0',
        'config': {
          'username': 'alice',
          'domain': 'relay.example.test',
          'publicBaseUrl': 'https://relay.example.test',
          'relayWs': 'wss://relay.example.test',
          'relayToken': 'relay-token-0123456789abcdef',
          'anchorActorUrl': null,
        },
      };
      final envelope =
          await IdentitySync.seal(package, 'pairing-code-0123456789');
      expect(envelope['v'], 2);
      expect(envelope['alg'], 'aes-256-gcm');
      expect(envelope['kdf'], 'hkdf-sha256');
      expect(envelope['salt_b64'], isNotEmpty);
      expect(envelope['nonce_b64'], isNotEmpty);
      expect(envelope['ciphertext_b64'], isNotEmpty);
      expect(envelope['mac_b64'], isNotEmpty);
      expect(IdentitySync.looksSealed(envelope), isTrue);

      final opened =
          await IdentitySync.open(envelope, 'pairing-code-0123456789');
      expect(opened, package);
    });

    test('relay bearer and pairing locator cannot decrypt identity packages',
        () async {
      final config = buildTestConfig();
      final secret = await IdentitySync.recoverySecret('synthetic-private-key');
      final envelope =
          await IdentitySync.seal({'private': 'synthetic-private-key'}, secret);
      await expectLater(IdentitySync.open(envelope, config.relayToken),
          throwsA(isA<SecretBoxAuthenticationError>()));
      expect((await IdentitySync.open(envelope, secret))['private'],
          'synthetic-private-key');
      const qrSecret = 'random-QR-only-secret-0123456789';
      final locator = await IdentitySync.pairingId(qrSecret);
      final paired =
          await IdentitySync.seal({'private': 'synthetic'}, qrSecret);
      await expectLater(IdentitySync.open(paired, locator),
          throwsA(isA<SecretBoxAuthenticationError>()));
      final client =
          FakeHttpClient(defaultResponse: FakeHttpResponse(statusCode: 204));
      await IdentitySync.uploadPairingPackage(
          relayBase: 'https://relay.example',
          code: qrSecret,
          user: 'alice',
          token: config.relayToken,
          envelope: paired,
          client: client);
      expect(client.lastRequest.url.pathSegments.last, locator);
      expect(client.lastRequest.url.toString(), isNot(contains(qrSecret)));
      expect(client.lastBody, isNot(contains(qrSecret)));
    });

    test('open rejects the wrong secret', () async {
      final envelope = await IdentitySync.seal(
        {'identity_private_key_pem': 'pem'},
        'correct-secret-0123456789',
      );
      await expectLater(
        IdentitySync.open(envelope, 'wrong-secret-0123456789'),
        throwsA(isA<SecretBoxAuthenticationError>()),
      );
    });

    test('open rejects a malformed envelope', () async {
      await expectLater(
        IdentitySync.open(<String, dynamic>{'v': 1}, 'secret'),
        throwsA(isA<FormatException>()),
      );
      await expectLater(
        IdentitySync.open(
          {
            'v': 1,
            'alg': 'aes-256-gcm',
            'kdf': 'hkdf-sha256',
            'salt_b64': '***',
            'nonce_b64': 'AAAA',
            'ciphertext_b64': 'AAAA',
            'mac_b64': 'AAAA',
          },
          'secret',
        ),
        throwsA(isA<FormatException>()),
      );
    });

    test('relay HTTP base is derived from ws/wss and keeps a custom port', () {
      expect(IdentitySync.relayHttpBaseFromWs('wss://relay.example.test/ws'),
          'https://relay.example.test');
      expect(IdentitySync.relayHttpBaseFromWs('ws://127.0.0.1:8787/ws'),
          'http://127.0.0.1:8787');
      expect(IdentitySync.relayHttpBaseFromWs('https://relay.test/path'),
          'https://relay.test');
      expect(IdentitySync.relayHttpBaseFromWs('not-a-url'), '');
    });

    test('buildPackage and configFromPackage carry account and identity data',
        () {
      final config = buildTestConfig(
        username: 'bob',
        relayToken: 'relay-token-0123456789abcdef',
        anchorActorUrl: 'https://anchor.example',
      ).copyWith(
        identityPrivateKeyPem: 'config-pem',
        chatIdentityPublicB64: 'config-public',
        chatIdentitySecretB64: 'config-secret',
      );
      final exported = {
        'public_key_pem': 'public',
        'private_key_pem': 'exported-pem',
        'chat_identity_public_b64': 'exported-public',
        'chat_identity_secret_b64': 'exported-secret',
      };
      final package = IdentitySync.buildPackage(config, exported);
      expect(package['identity_private_key_pem'], 'exported-pem');
      expect(package['chat_identity_public_b64'], 'exported-public');
      expect(package['chat_identity_secret_b64'], 'exported-secret');
      final account = package['config'] as Map<String, dynamic>;
      expect(account['username'], 'bob');
      expect(account['publicBaseUrl'], config.publicBaseUrl);
      expect(account['relayWs'], config.relayWs);
      expect(account['relayToken'], config.relayToken);
      expect(account['anchorActorUrl'], 'https://anchor.example');

      final rebuilt = IdentitySync.configFromPackage(package);
      expect(rebuilt.username, 'bob');
      expect(rebuilt.publicBaseUrl, config.publicBaseUrl);
      expect(rebuilt.relayWs, config.relayWs);
      expect(rebuilt.relayToken, config.relayToken);
      expect(rebuilt.bind, '127.0.0.1:8788');
      expect(rebuilt.internalToken, isNotEmpty);
      expect(rebuilt.identityPrivateKeyPem, 'exported-pem');
      expect(rebuilt.chatIdentityPublicB64, 'exported-public');
      expect(rebuilt.chatIdentitySecretB64, 'exported-secret');
      expect(rebuilt.deviceSyncEnable, isTrue);
    });

    test('the package carries the account profile to the linked client', () {
      final config = buildTestConfig().copyWith(
        displayName: 'Bob',
        summary: '<p>hello</p>',
        iconUrl: 'https://example.test/avatar.png',
        iconMediaType: 'image/png',
        imageUrl: 'https://example.test/banner.png',
        imageMediaType: 'image/jpeg',
        profileFields: [ProfileFieldKV(name: 'site', value: 'https://bob')],
        manuallyApprovesFollowers: true,
        blockedDomains: const ['spam.example'],
        blockedActors: const ['https://bad.example/users/evil'],
        apRelays: const ['https://relay.misskey.io/actor'],
        postDeliveryMode: CoreConfig.postDeliveryP2pOnly,
      );
      final package = IdentitySync.buildPackage(config, const {});
      final rebuilt = IdentitySync.configFromPackage(package);

      expect(rebuilt.displayName, 'Bob');
      expect(rebuilt.summary, '<p>hello</p>');
      expect(rebuilt.iconUrl, 'https://example.test/avatar.png');
      expect(rebuilt.iconMediaType, 'image/png');
      expect(rebuilt.imageUrl, 'https://example.test/banner.png');
      expect(rebuilt.imageMediaType, 'image/jpeg');
      expect(rebuilt.profileFields.single.name, 'site');
      expect(rebuilt.profileFields.single.value, 'https://bob');
      expect(rebuilt.manuallyApprovesFollowers, isTrue);
      expect(rebuilt.blockedDomains, ['spam.example']);
      expect(rebuilt.blockedActors, ['https://bad.example/users/evil']);
      expect(rebuilt.apRelays, ['https://relay.misskey.io/actor']);
      expect(rebuilt.postDeliveryMode, CoreConfig.postDeliveryP2pOnly);
    });

    test('mergeIdentity also restores the account profile', () {
      final config = buildTestConfig();
      final package = IdentitySync.buildPackage(
        buildTestConfig().copyWith(
          displayName: 'Bob',
          iconUrl: 'https://example.test/avatar.png',
          blockedDomains: const ['spam.example'],
        ),
        const {},
      );
      final merged = IdentitySync.mergeIdentity(config, package);
      expect(merged.displayName, 'Bob');
      expect(merged.iconUrl, 'https://example.test/avatar.png');
      expect(merged.blockedDomains, ['spam.example']);
      // Untouched local values survive when the package has no value.
      expect(merged.username, config.username);
      expect(merged.relayToken, config.relayToken);
    });

    test('buildPackage includes a non-empty chat snapshot', () async {
      final config = buildTestConfig();
      final exported = {
        'private_key_pem': 'pem',
        'chat_identity_public_b64': 'public',
        'chat_identity_secret_b64': 'secret',
      };
      final snapshot = <String, dynamic>{
        'v': 1,
        'threads': [
          {'thread_id': 't1'}
        ],
        'members': <dynamic>[],
        'messages': [
          {'message_id': 'm1', 'thread_id': 't1'}
        ],
        'statuses': <dynamic>[],
        'reactions': <dynamic>[],
        'pins': <dynamic>[],
        'call_signals': <dynamic>[],
        'truncated': false,
      };
      final package = IdentitySync.buildPackage(
        config,
        exported,
        chatSnapshot: snapshot,
      );
      expect(package['chat_snapshot'], snapshot);
      // configFromPackage ignores the snapshot: it is not part of CoreConfig.
      expect(IdentitySync.configFromPackage(package).username, config.username);

      final envelope = await IdentitySync.seal(package, 'pairing-secret-1234');
      final opened = await IdentitySync.open(envelope, 'pairing-secret-1234');
      expect(opened['chat_snapshot'], snapshot);
    });

    test('buildPackage omits an absent or empty chat snapshot', () {
      final config = buildTestConfig();
      final exported = {
        'private_key_pem': 'pem',
        'chat_identity_public_b64': 'public',
        'chat_identity_secret_b64': 'secret',
      };
      final withoutSnapshot = IdentitySync.buildPackage(config, exported);
      expect(withoutSnapshot.containsKey('chat_snapshot'), isFalse);
      final emptySnapshot = IdentitySync.buildPackage(
        config,
        exported,
        chatSnapshot: const <String, dynamic>{},
      );
      expect(emptySnapshot.containsKey('chat_snapshot'), isFalse);
    });

    test('mergeIdentity fills only the identity fields', () {
      final config = buildTestConfig(username: 'carol');
      final merged = IdentitySync.mergeIdentity(config, {
        'identity_private_key_pem': 'pem',
        'chat_identity_public_b64': 'public',
        'chat_identity_secret_b64': 'secret',
        'config': {'username': 'ignored'},
      });
      expect(merged.username, 'carol');
      expect(merged.identityPrivateKeyPem, 'pem');
      expect(merged.chatIdentityPublicB64, 'public');
      expect(merged.chatIdentitySecretB64, 'secret');
    });

    test('relay identity fetch/upload use Bearer auth and the expected codes',
        () async {
      final client =
          FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
      client.route('GET', '/_fedi3/users/alice/identity',
          FakeHttpResponse(statusCode: 404, body: ''));
      final missing = await IdentitySync.fetchRelayIdentity(
        relayBase: 'https://relay.example.test',
        user: 'alice',
        token: 'relay-token-0123456789abcdef',
        client: client,
      );
      expect(missing, isNull);

      client.json('GET', '/_fedi3/users/alice/identity', {'v': 1});
      final found = await IdentitySync.fetchRelayIdentity(
        relayBase: 'https://relay.example.test/',
        user: 'alice',
        token: 'relay-token-0123456789abcdef',
        client: client,
      );
      expect(found, {'v': 1});
      expect(client.lastHeader('Authorization'),
          'Bearer relay-token-0123456789abcdef');

      client.route('PUT', '/_fedi3/users/alice/identity',
          FakeHttpResponse(statusCode: 204, body: ''));
      await IdentitySync.uploadRelayIdentity(
        relayBase: 'https://relay.example.test',
        user: 'alice',
        token: 'relay-token-0123456789abcdef',
        envelope: {'v': 1},
        client: client,
      );
      expect(client.lastHeader('Authorization'),
          'Bearer relay-token-0123456789abcdef');
      expect(jsonDecode(client.lastBody), {'v': 1});

      client.route('PUT', '/_fedi3/users/alice/identity',
          FakeHttpResponse(statusCode: 200, body: '{}'));
      await expectLater(
        IdentitySync.uploadRelayIdentity(
          relayBase: 'https://relay.example.test',
          user: 'alice',
          token: 'relay-token-0123456789abcdef',
          envelope: {'v': 1},
          client: client,
        ),
        throwsA(isA<StateError>()),
      );
    });

    test('pairing upload targets the code and user with Bearer auth', () async {
      final client =
          FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
      final pairingPath =
          '/_fedi3/pair/${await IdentitySync.pairingId('abcd1234')}';
      client.route(
          'PUT', pairingPath, FakeHttpResponse(statusCode: 204, body: ''));
      await IdentitySync.uploadPairingPackage(
        relayBase: 'https://relay.example.test',
        code: 'abcd1234',
        user: 'alice',
        token: 'relay-token-0123456789abcdef',
        envelope: {'v': 1},
        client: client,
      );
      expect(client.lastRequest.url.queryParameters['user'], 'alice');
      expect(client.lastHeader('Authorization'),
          'Bearer relay-token-0123456789abcdef');

      client.route(
          'GET', pairingPath, FakeHttpResponse(statusCode: 410, body: ''));
      expect(
        await IdentitySync.fetchPairingPackage(
          relayBase: 'https://relay.example.test',
          code: 'abcd1234',
          client: client,
        ),
        isNull,
      );
    });

    test(
        'ensurePublished encrypts the local identity with an independent secret',
        () async {
      final client =
          FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}'));
      CoreHttp.useTestClient(client);
      addTearDown(CoreHttp.reset);
      final config = buildTestConfig();
      client.json('GET', '/_fedi3/users/alice/identity',
          {'v': 1, 'ciphertext_b64': 'legacy'});
      client.json('GET', '/_fedi3/identity/export', {
        'public_key_pem': 'public',
        'private_key_pem': 'private',
        'chat_identity_public_b64': 'chat-public',
        'chat_identity_secret_b64': 'chat-secret',
      });
      client.route('PUT', '/_fedi3/users/alice/identity',
          FakeHttpResponse(statusCode: 204, body: ''));

      await IdentitySync.ensurePublished(config);

      final puts = client.requestsFor('PUT', '/_fedi3/users/alice/identity');
      expect(puts, hasLength(1));
      final envelope =
          jsonDecode(client.requestBodies[client.requestBodies.length - 1])
              as Map<String, dynamic>;
      expect(IdentitySync.looksSealed(envelope), isTrue);
      final opened = await IdentitySync.open(
          envelope, await IdentitySync.recoverySecret('private'));
      expect(opened['identity_private_key_pem'], 'private');
      expect(opened['chat_identity_public_b64'], 'chat-public');
      expect(opened['chat_identity_secret_b64'], 'chat-secret');
    });
  });
}
