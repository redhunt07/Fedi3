/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'package:cryptography/cryptography.dart';

import '../model/core_config.dart';
import '../model/ui_prefs.dart';

class BackupBundle {
  BackupBundle({required this.config, required this.prefs});

  final CoreConfig config;
  final UiPrefs prefs;
}

class BackupCodec {
  static const int version = 1;

  static String encode({required CoreConfig config, required UiPrefs prefs}) {
    final data = {
      'v': version,
      'coreConfig': config.toJson(),
      'uiPrefs': prefs.toJson(),
    };
    return const JsonEncoder.withIndent('  ').convert(data);
  }

  static BackupBundle decode(String raw) {
    final txt = raw.trim();
    if (txt.isEmpty) throw const FormatException('empty');
    final json = jsonDecode(txt);
    if (json is! Map) throw const FormatException('invalid json');

    // AM-06/H-56: reject backups written by a newer app instead of silently
    // dropping unknown fields.
    final rawVersion = json['v'];
    if (rawVersion is! num) {
      throw const FormatException('missing backup version');
    }
    final version = rawVersion.toInt();
    if (version > BackupCodec.version) {
      throw FormatException(
          'unsupported backup version: $version (max ${BackupCodec.version})');
    }

    final cfgJson = (json['coreConfig'] as Map?)?.cast<String, dynamic>();
    final prefsJson = (json['uiPrefs'] as Map?)?.cast<String, dynamic>();
    if (cfgJson == null || prefsJson == null) {
      throw const FormatException('missing sections');
    }

    return BackupBundle(
      config: CoreConfig.fromJson(cfgJson),
      prefs: UiPrefs.fromJson(prefsJson),
    );
  }

  static const _iterations = 600000;
  static const _aad = [70, 101, 100, 105, 51, 45, 118, 50];

  static Future<String> encodeEncrypted(
      {required CoreConfig config,
      required UiPrefs prefs,
      required String passphrase}) async {
    if (passphrase.length < 12) {
      throw const FormatException('Use a passphrase of at least 12 characters');
    }
    final algorithm = AesGcm.with256bits();
    final salt = algorithm.newNonce() + algorithm.newNonce().take(4).toList();
    final key = await Pbkdf2(
            macAlgorithm: Hmac.sha256(), iterations: _iterations, bits: 256)
        .deriveKey(secretKey: SecretKey(utf8.encode(passphrase)), nonce: salt);
    final box = await algorithm.encrypt(
        utf8.encode(encode(config: config, prefs: prefs)),
        secretKey: key,
        aad: _aad);
    return jsonEncode({
      'v': 2,
      'scope': 'settings',
      'kdf': 'pbkdf2-sha256',
      'iterations': _iterations,
      'salt': base64Encode(salt),
      'nonce': base64Encode(box.nonce),
      'ciphertext': base64Encode(box.cipherText),
      'mac': base64Encode(box.mac.bytes)
    });
  }

  static Future<BackupBundle> decodeEncrypted(String raw,
      {required String passphrase}) async {
    if (raw.length > 8 * 1024 * 1024) {
      throw const FormatException('Backup too large');
    }
    final envelope = jsonDecode(raw) as Map;
    if (envelope['v'] == 1) return decode(raw);
    if (envelope['v'] != 2 ||
        envelope['kdf'] != 'pbkdf2-sha256' ||
        envelope['iterations'] != _iterations ||
        envelope['scope'] != 'settings') {
      throw const FormatException('Unsupported backup format');
    }
    List<int> field(String name) => base64Decode(envelope[name] as String);
    final salt = field('salt'), nonce = field('nonce'), mac = field('mac');
    if (salt.length != 16 || nonce.length != 12 || mac.length != 16) {
      throw const FormatException('Invalid backup parameters');
    }
    try {
      final key = await Pbkdf2(
              macAlgorithm: Hmac.sha256(), iterations: _iterations, bits: 256)
          .deriveKey(
              secretKey: SecretKey(utf8.encode(passphrase)), nonce: salt);
      final clear = await AesGcm.with256bits().decrypt(
          SecretBox(field('ciphertext'), nonce: nonce, mac: Mac(mac)),
          secretKey: key,
          aad: _aad);
      return decode(utf8.decode(clear));
    } on SecretBoxAuthenticationError {
      throw const FormatException('Wrong passphrase or damaged backup');
    }
  }

  static String suggestedFileName(CoreConfig config) {
    String safe(String s) {
      final v = s.trim().replaceAll(RegExp(r'[^a-zA-Z0-9._-]+'), '_');
      return v.isEmpty ? 'unknown' : v;
    }

    final user = safe(config.username);
    final domain = safe(config.domain);
    return 'fedi3-backup-$user@$domain.json';
  }
}
