/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:math';

import 'package:cryptography/cryptography.dart';
import 'package:http/http.dart' as http;

import '../core/core_api.dart';
import '../core/core_http.dart';
import '../model/core_config.dart';

/// Sealed account identity transfer for multi-device login and QR pairing.
///
/// The relay only stores the sealed envelope: the AES-256-GCM key is derived
/// with HKDF-SHA256 from the shared secret (the independent recovery key for the
/// account blob, the QR-only secret for a pairing session), a random salt and the
/// `fedi3-identity-v1` info string.
class IdentitySync {
  IdentitySync._();

  static const String envelopeVersion = '2';
  static const String algorithm = 'aes-256-gcm';
  static const String kdf = 'hkdf-sha256';
  static const String kdfInfo = 'fedi3-identity-v1';

  static final AesGcm _cipher = AesGcm.with256bits();
  static final Hkdf _hkdf = Hkdf(hmac: Hmac.sha256(), outputLength: 32);

  /// Derives the relay HTTP origin from the configured relay WebSocket URL.
  /// `ws://` becomes `http://` and `wss://` becomes `https://`; paths, query
  /// strings and default ports are dropped.
  static String relayHttpBaseFromWs(String relayWs) {
    final raw = relayWs.trim();
    if (raw.isEmpty) return '';
    final uri = Uri.tryParse(raw);
    if (uri == null || uri.host.isEmpty) return '';
    final String scheme;
    switch (uri.scheme.toLowerCase()) {
      case 'ws':
      case 'http':
        scheme = 'http';
        break;
      case 'wss':
      case 'https':
        scheme = 'https';
        break;
      default:
        return '';
    }
    final defaultPort = scheme == 'https' ? 443 : 80;
    final port = uri.hasPort && uri.port != defaultPort ? ':${uri.port}' : '';
    final host = uri.host.contains(':') ? '[${uri.host}]' : uri.host;
    return '$scheme://$host$port';
  }

  /// Only its hash is sent to the relay. The QR retains the random secret.
  static Future<String> pairingId(String secret) async => base64UrlEncode(
          (await Sha256().hash(utf8.encode('fedi3-pair-id/v2:$secret'))).bytes)
      .replaceAll('=', '');

  /// Stable recovery key available only to clients holding the private identity.
  /// It must never be used as an HTTP credential or sent in a URL.
  static Future<String> recoverySecret(String privatePem) async {
    if (privatePem.trim().isEmpty) throw StateError('Identity required');
    final normalized = privatePem.replaceAll(RegExp(r'\s+'), '');
    return base64UrlEncode((await Sha256()
                .hash(utf8.encode('fedi3-identity-recovery/v2:$normalized')))
            .bytes)
        .replaceAll('=', '');
  }

  static Future<String> localRecoverySecret(CoreConfig config) async {
    final identity = await CoreApi(config: config).exportIdentity();
    return recoverySecret(identity['private_key_pem'] as String);
  }

  static String _normalizeBase(String base) =>
      base.trim().replaceAll(RegExp(r'/+$'), '');

  static String _b64(List<int> bytes) => base64Encode(bytes);

  static List<int> _unb64(Object? value, String field) {
    if (value is! String || value.trim().isEmpty) {
      throw FormatException('identity envelope is missing $field');
    }
    try {
      return base64Decode(value.trim());
    } on FormatException {
      throw FormatException('identity envelope has an invalid $field');
    }
  }

  static bool looksSealed(Map<String, dynamic> envelope) =>
      (envelope['v']?.toString() == envelopeVersion || envelope['v'] == 1) &&
      envelope['alg']?.toString() == algorithm &&
      envelope['kdf']?.toString() == kdf &&
      envelope['ciphertext_b64'] is String &&
      envelope['nonce_b64'] is String &&
      envelope['mac_b64'] is String &&
      envelope['salt_b64'] is String;

  /// Seals [package] (identity keys and account config) with [secret].
  static Future<Map<String, dynamic>> seal(
    Map<String, dynamic> package,
    String secret,
  ) async {
    if (secret.trim().isEmpty) {
      throw ArgumentError('identity secret is empty');
    }
    final random = Random.secure();
    final salt = List<int>.generate(32, (_) => random.nextInt(256));
    final key = await _deriveKey(secret, salt);
    final nonce = _cipher.newNonce();
    final box = await _cipher.encrypt(
      utf8.encode(jsonEncode(package)),
      secretKey: SecretKey(key),
      nonce: nonce,
    );
    return {
      'v': 2,
      'alg': algorithm,
      'kdf': kdf,
      'salt_b64': _b64(salt),
      'nonce_b64': _b64(box.nonce),
      'ciphertext_b64': _b64(box.cipherText),
      'mac_b64': _b64(box.mac.bytes),
    };
  }

  /// Opens an envelope produced by [seal]. Throws [SecretBoxAuthenticationError]
  /// when the secret is wrong and [FormatException] for a malformed envelope.
  static Future<Map<String, dynamic>> open(
    Map<String, dynamic> envelope,
    String secret,
  ) async {
    if (secret.trim().isEmpty) {
      throw ArgumentError('identity secret is empty');
    }
    if (!looksSealed(envelope)) {
      throw const FormatException('invalid identity envelope');
    }
    final salt = _unb64(envelope['salt_b64'], 'salt_b64');
    final nonce = _unb64(envelope['nonce_b64'], 'nonce_b64');
    final cipherText = _unb64(envelope['ciphertext_b64'], 'ciphertext_b64');
    final mac = Mac(_unb64(envelope['mac_b64'], 'mac_b64'));
    final key = await _deriveKey(secret, salt);
    final clear = await _cipher.decrypt(
      SecretBox(cipherText, nonce: nonce, mac: mac),
      secretKey: SecretKey(key),
    );
    final decoded = jsonDecode(utf8.decode(clear));
    if (decoded is! Map) {
      throw const FormatException('identity package is not a JSON object');
    }
    return decoded.cast<String, dynamic>();
  }

  static Future<List<int>> _deriveKey(String secret, List<int> salt) async {
    final derived = await _hkdf.deriveKey(
      secretKey: SecretKey(utf8.encode(secret)),
      nonce: salt,
      info: utf8.encode(kdfInfo),
    );
    return derived.bytes;
  }

  /// Builds the identity package transferred to another client.
  static Map<String, dynamic> buildPackage(
    CoreConfig config,
    Map<String, dynamic> exported, {
    Map<String, dynamic>? chatSnapshot,
  }) {
    String value(Object? primary, Object? fallback) {
      final first = primary?.toString().trim() ?? '';
      if (first.isNotEmpty) return first;
      return fallback?.toString().trim() ?? '';
    }

    return {
      'identity_private_key_pem': value(
        exported['private_key_pem'],
        config.identityPrivateKeyPem,
      ),
      'chat_identity_public_b64': value(
        exported['chat_identity_public_b64'],
        config.chatIdentityPublicB64,
      ),
      'chat_identity_secret_b64': value(
        exported['chat_identity_secret_b64'],
        config.chatIdentitySecretB64,
      ),
      // Optional: the identity blob stays small when there is no history.
      if (chatSnapshot != null && chatSnapshot.isNotEmpty)
        'chat_snapshot': chatSnapshot,
      'config': {
        'username': config.username,
        'domain': config.domain,
        'publicBaseUrl': config.publicBaseUrl,
        'relayWs': config.relayWs,
        'relayToken': config.relayToken,
        'anchorActorUrl': config.anchorActorUrl,
        // Profile and moderation settings belong to the account: a linked
        // client must look and behave like the original one.
        'displayName': config.displayName,
        'summary': config.summary,
        'iconUrl': config.iconUrl,
        'iconMediaType': config.iconMediaType,
        'imageUrl': config.imageUrl,
        'imageMediaType': config.imageMediaType,
        'profileFields': config.profileFields.map((f) => f.toJson()).toList(),
        'manuallyApprovesFollowers': config.manuallyApprovesFollowers,
        'blockedDomains': config.blockedDomains,
        'blockedActors': config.blockedActors,
        'apRelays': config.apRelays,
        'postDeliveryMode': config.postDeliveryMode,
      },
    };
  }

  /// Builds a per-device configuration from a received identity package.
  /// Bind address and internal token are local values and are not transferred.
  static CoreConfig configFromPackage(Map<String, dynamic> package) {
    final rawConfig = package['config'];
    if (rawConfig is! Map) {
      throw const FormatException('identity package has no config');
    }
    final config = rawConfig.cast<String, dynamic>();
    final username = config['username']?.toString().trim() ?? '';
    final publicBaseUrl = config['publicBaseUrl']?.toString().trim() ?? '';
    final relayWs = config['relayWs']?.toString().trim() ?? '';
    final relayToken = config['relayToken']?.toString().trim() ?? '';
    if (username.isEmpty ||
        publicBaseUrl.isEmpty ||
        relayWs.isEmpty ||
        relayToken.isEmpty) {
      throw const FormatException('identity package config is incomplete');
    }
    final domain = config['domain']?.toString().trim() ?? '';
    String text(Object? raw) => raw?.toString().trim() ?? '';
    List<String> strings(Object? raw) => raw is List
        ? raw
            .map((item) => item.toString().trim())
            .where((item) => item.isNotEmpty)
            .toList(growable: false)
        : const <String>[];
    List<ProfileFieldKV> fields(Object? raw) => raw is List
        ? raw
            .whereType<Map>()
            .map((item) => ProfileFieldKV.fromJson(
                item.map((key, value) => MapEntry(key.toString(), value))))
            .where((field) => field.name.isNotEmpty)
            .toList(growable: false)
        : const <ProfileFieldKV>[];
    final postDeliveryMode = text(config['postDeliveryMode']);
    return CoreConfig(
      username: username,
      domain: domain.isEmpty ? Uri.parse(publicBaseUrl).host : domain,
      publicBaseUrl: publicBaseUrl,
      relayWs: relayWs,
      relayToken: relayToken,
      bind: '127.0.0.1:8788',
      internalToken: CoreConfig.randomToken(),
      p2pEnable: true,
      apRelays: strings(config['apRelays']),
      displayName: text(config['displayName']),
      summary: text(config['summary']),
      iconUrl: text(config['iconUrl']),
      iconMediaType: text(config['iconMediaType']),
      imageUrl: text(config['imageUrl']),
      imageMediaType: text(config['imageMediaType']),
      profileFields: fields(config['profileFields']),
      manuallyApprovesFollowers: config['manuallyApprovesFollowers'] == true,
      blockedDomains: strings(config['blockedDomains']),
      blockedActors: strings(config['blockedActors']),
      postDeliveryMode: postDeliveryMode.isEmpty
          ? CoreConfig.postDeliveryP2pRelay
          : postDeliveryMode,
      identityPrivateKeyPem:
          package['identity_private_key_pem']?.toString().trim(),
      chatIdentityPublicB64:
          package['chat_identity_public_b64']?.toString().trim(),
      chatIdentitySecretB64:
          package['chat_identity_secret_b64']?.toString().trim(),
      anchorActorUrl: config['anchorActorUrl']?.toString(),
    );
  }

  /// Merges the identity fields of [package] into an existing configuration.
  static CoreConfig mergeIdentity(
    CoreConfig config,
    Map<String, dynamic> package,
  ) {
    String? value(Object? raw) {
      final text = raw?.toString().trim() ?? '';
      return text.isEmpty ? null : text;
    }

    List<String>? strings(Object? raw) {
      if (raw is! List) return null;
      final list = raw
          .map((item) => item.toString().trim())
          .where((item) => item.isNotEmpty)
          .toList(growable: false);
      return list.isEmpty ? null : list;
    }

    List<ProfileFieldKV>? fields(Object? raw) {
      if (raw is! List) return null;
      final list = raw
          .whereType<Map>()
          .map((item) => ProfileFieldKV.fromJson(
              item.map((key, value) => MapEntry(key.toString(), value))))
          .where((field) => field.name.isNotEmpty)
          .toList(growable: false);
      return list.isEmpty ? null : list;
    }

    final rawConfig = package['config'];
    final shared = rawConfig is Map
        ? rawConfig.map((key, value) => MapEntry(key.toString(), value))
        : const <String, dynamic>{};
    final approved = shared['manuallyApprovesFollowers'];
    return config.copyWith(
      identityPrivateKeyPem: value(package['identity_private_key_pem']) ??
          config.identityPrivateKeyPem,
      chatIdentityPublicB64: value(package['chat_identity_public_b64']) ??
          config.chatIdentityPublicB64,
      chatIdentitySecretB64: value(package['chat_identity_secret_b64']) ??
          config.chatIdentitySecretB64,
      // The published identity blob also carries the account profile, so a
      // manual login on another client picks it up.
      displayName: value(shared['displayName']) ?? config.displayName,
      summary: value(shared['summary']) ?? config.summary,
      iconUrl: value(shared['iconUrl']) ?? config.iconUrl,
      iconMediaType: value(shared['iconMediaType']) ?? config.iconMediaType,
      imageUrl: value(shared['imageUrl']) ?? config.imageUrl,
      imageMediaType: value(shared['imageMediaType']) ?? config.imageMediaType,
      profileFields: fields(shared['profileFields']) ?? config.profileFields,
      manuallyApprovesFollowers:
          approved is bool ? approved : config.manuallyApprovesFollowers,
      blockedDomains:
          strings(shared['blockedDomains']) ?? config.blockedDomains,
      blockedActors: strings(shared['blockedActors']) ?? config.blockedActors,
      apRelays: strings(shared['apRelays']) ?? config.apRelays,
      postDeliveryMode:
          value(shared['postDeliveryMode']) ?? config.postDeliveryMode,
    );
  }

  static Uri _identityUri(String relayBase, String user) {
    final base = _normalizeBase(relayBase);
    if (base.isEmpty) throw ArgumentError('relay base is empty');
    return Uri.parse(
        '$base/_fedi3/users/${Uri.encodeComponent(user.trim())}/identity');
  }

  static Uri _pairUri(String relayBase, String code, {String? user}) {
    final base = _normalizeBase(relayBase);
    if (base.isEmpty) throw ArgumentError('relay base is empty');
    final uri = Uri.parse('$base/_fedi3/pair/${Uri.encodeComponent(code)}');
    if (user == null || user.trim().isEmpty) return uri;
    return uri.replace(queryParameters: {'user': user.trim()});
  }

  static Future<Map<String, dynamic>> _decode(
      http.Response resp, String operation) async {
    final text = resp.body.trim();
    if (text.isEmpty) return <String, dynamic>{};
    final decoded = jsonDecode(text);
    if (decoded is! Map) {
      throw FormatException('$operation returned a non-object body');
    }
    return decoded.cast<String, dynamic>();
  }

  /// GET /_fedi3/users/:user/identity. Returns null when the relay has no blob.
  static Future<Map<String, dynamic>?> fetchRelayIdentity({
    required String relayBase,
    required String user,
    required String token,
    http.Client? client,
  }) async {
    final uri = _identityUri(relayBase, user);
    final resp = await (client ?? CoreHttp.client)
        .get(uri, headers: _bearer(token))
        .timeout(CoreHttp.totalTimeout);
    if (resp.statusCode == 404 || resp.statusCode == 410) return null;
    if (resp.statusCode != 200) {
      throw StateError(
          'identity fetch failed: ${resp.statusCode} ${resp.body.trim()}');
    }
    return _decode(resp, 'identity fetch');
  }

  /// PUT /_fedi3/users/:user/identity. The relay replies with 204.
  static Future<void> uploadRelayIdentity({
    required String relayBase,
    required String user,
    required String token,
    required Map<String, dynamic> envelope,
    http.Client? client,
  }) async {
    final uri = _identityUri(relayBase, user);
    final resp = await (client ?? CoreHttp.client)
        .put(
          uri,
          headers: {..._bearer(token), 'Content-Type': 'application/json'},
          body: jsonEncode(envelope),
        )
        .timeout(CoreHttp.totalTimeout);
    if (resp.statusCode != 204) {
      throw StateError(
          'identity upload failed: ${resp.statusCode} ${resp.body.trim()}');
    }
  }

  /// GET /_fedi3/pair/:code (unauthenticated; the code is the secret).
  /// Returns null when the code is unknown or expired.
  static Future<Map<String, dynamic>?> fetchPairingPackage({
    required String relayBase,
    required String code,
    http.Client? client,
  }) async {
    final uri = _pairUri(relayBase, await pairingId(code));
    final resp = await (client ?? CoreHttp.client)
        .get(uri)
        .timeout(CoreHttp.totalTimeout);
    if (resp.statusCode == 404 || resp.statusCode == 410) return null;
    if (resp.statusCode != 200) {
      throw StateError(
          'pairing fetch failed: ${resp.statusCode} ${resp.body.trim()}');
    }
    return _decode(resp, 'pairing fetch');
  }

  /// PUT /_fedi3/pair/:code?user=:user. The relay replies with 204.
  static Future<void> uploadPairingPackage({
    required String relayBase,
    required String code,
    required String user,
    required String token,
    required Map<String, dynamic> envelope,
    http.Client? client,
  }) async {
    final uri = _pairUri(relayBase, await pairingId(code), user: user);
    final resp = await (client ?? CoreHttp.client)
        .put(
          uri,
          headers: {..._bearer(token), 'Content-Type': 'application/json'},
          body: jsonEncode(envelope),
        )
        .timeout(CoreHttp.totalTimeout);
    if (resp.statusCode != 204) {
      throw StateError(
          'pairing upload failed: ${resp.statusCode} ${resp.body.trim()}');
    }
  }

  static Map<String, String> _bearer(String token) => {
        if (token.trim().isNotEmpty) 'Authorization': 'Bearer ${token.trim()}',
      };

  /// Best-effort: make sure the relay stores the account identity envelope.
  /// Runs after a successful core start; never throws.
  static Future<void> ensurePublished(CoreConfig config) async {
    try {
      final relayBase = relayHttpBaseFromWs(config.relayWs);
      if (relayBase.isEmpty) return;
      final existing = await fetchRelayIdentity(
        relayBase: relayBase,
        user: config.username,
        token: config.relayToken,
      );
      if (existing?['v'] == 2) return;
      final exported = await CoreApi(config: config).exportIdentity();
      final privateKey = exported['private_key_pem']?.toString().trim() ?? '';
      if (privateKey.isEmpty) return;
      final package = buildPackage(config, exported);
      final envelope = await seal(package, await recoverySecret(privateKey));
      await uploadRelayIdentity(
        relayBase: relayBase,
        user: config.username,
        token: config.relayToken,
        envelope: envelope,
      );
    } catch (_) {
      // Identity publishing is best-effort and must never block the UI.
    }
  }
}
