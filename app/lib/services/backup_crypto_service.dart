/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:pointycastle/export.dart';

class BackupCiphertext {
  BackupCiphertext({
    required this.saltB64,
    required this.nonceB64,
    required this.cipherB64,
    required this.tagB64,
  });

  final String saltB64;
  final String nonceB64;
  final String cipherB64;
  final String tagB64;
}

class BackupCryptoService {
  static const int _keySize = 32;
  static const int _nonceSize = 12;
  static const int _tagBytes = 16;
  static const String _aad = 'fedi3-backup-v1';

  BackupCiphertext encrypt(Uint8List plaintext, String token) {
    final salt = _randomBytes(16);
    final key = _deriveKey(token, salt);
    final nonce = _randomBytes(_nonceSize);
    final cipher = GCMBlockCipher(AESEngine());
    cipher.init(
      true,
      AEADParameters(
        KeyParameter(key),
        _tagBytes * 8,
        nonce,
        Uint8List.fromList(utf8.encode(_aad)),
      ),
    );
    final out = cipher.process(plaintext);
    final tag = out.sublist(out.length - _tagBytes);
    final ciphertext = out.sublist(0, out.length - _tagBytes);
    return BackupCiphertext(
      saltB64: base64Encode(salt),
      nonceB64: base64Encode(nonce),
      cipherB64: base64Encode(ciphertext),
      tagB64: base64Encode(tag),
    );
  }

  Uint8List decrypt({
    required String saltB64,
    required String nonceB64,
    required String cipherB64,
    required String tagB64,
    required String token,
  }) {
    final salt = base64Decode(saltB64);
    final nonce = base64Decode(nonceB64);
    final ciphertext = base64Decode(cipherB64);
    final tag = base64Decode(tagB64);
    final key = _deriveKey(token, salt);
    final cipher = GCMBlockCipher(AESEngine());
    cipher.init(
      false,
      AEADParameters(
        KeyParameter(key),
        _tagBytes * 8,
        nonce,
        Uint8List.fromList(utf8.encode(_aad)),
      ),
    );
    final combined = Uint8List(ciphertext.length + tag.length)
      ..setRange(0, ciphertext.length, ciphertext)
      ..setRange(ciphertext.length, ciphertext.length + tag.length, tag);
    return cipher.process(combined);
  }

  Uint8List _deriveKey(String token, Uint8List salt) {
    final hkdf = HKDFKeyDerivator(SHA256Digest());
    hkdf.init(
      HkdfParameters(
        Uint8List.fromList(utf8.encode(token)),
        _keySize,
        salt,
        Uint8List.fromList(utf8.encode(_aad)),
      ),
    );
    final out = Uint8List(_keySize);
    hkdf.deriveKey(null, 0, out, 0);
    return out;
  }

  Uint8List _randomBytes(int len) {
    final rnd = Random.secure();
    final out = Uint8List(len);
    for (var i = 0; i < len; i++) {
      out[i] = rnd.nextInt(256);
    }
    return out;
  }
}
