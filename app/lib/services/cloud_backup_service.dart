/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

import '../core/core_api.dart';
import '../model/core_config.dart';
import '../model/ui_prefs.dart';
import 'backup_crypto_service.dart';
import 'encryption_manager.dart';

class CloudBackupMeta {
  CloudBackupMeta({
    required this.updatedAtMs,
    required this.sizeBytes,
    required this.contentType,
  });

  final int updatedAtMs;
  final int sizeBytes;
  final String contentType;
}

class CloudBackupPackage {
  CloudBackupPackage({
    required this.config,
    required this.prefs,
    required this.encryptionKeys,
    required this.coreBackup,
  });

  final CoreConfig config;
  final UiPrefs prefs;
  final Map<String, String> encryptionKeys;
  final Map<String, dynamic> coreBackup;
}

class CloudBackupService {
  CloudBackupService({required this.config});

  final CoreConfig config;

  Uri _relayUri(String path, [Map<String, String>? query]) {
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '');
    return Uri.parse(base).replace(path: path, queryParameters: query);
  }

  Future<CloudBackupMeta?> fetchMeta() async {
    final uri = _relayUri('/_fedi3/backup', {'username': config.username});
    final resp = await http.get(uri, headers: _authHeaders());
    if (resp.statusCode == 404) return null;
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('backup meta failed: ${resp.statusCode} ${resp.body}');
    }
    final json = jsonDecode(resp.body) as Map<String, dynamic>;
    return CloudBackupMeta(
      updatedAtMs: (json['updated_at_ms'] as num?)?.toInt() ?? 0,
      sizeBytes: (json['size_bytes'] as num?)?.toInt() ?? 0,
      contentType: (json['content_type'] as String?)?.trim() ?? '',
    );
  }

  Future<void> upload({required UiPrefs prefs}) async {
    final api = CoreApi(config: config);
    final coreBackup = await api.exportBackup();
    final keys = await EncryptionManager().exportKeys();

    final payload = {
      'v': 1,
      'created_at_ms': DateTime.now().millisecondsSinceEpoch,
      'config': config.toJson(),
      'uiPrefs': prefs.toJson(),
      'encryptionKeys': keys,
      'coreBackup': coreBackup,
    };
    final plain = Uint8List.fromList(utf8.encode(jsonEncode(payload)));
    final crypto = BackupCryptoService();
    final enc = crypto.encrypt(plain, config.relayToken);
    final envelope = {
      'v': 1,
      'created_at_ms': DateTime.now().millisecondsSinceEpoch,
      'salt_b64': enc.saltB64,
      'nonce_b64': enc.nonceB64,
      'cipher_b64': enc.cipherB64,
      'tag_b64': enc.tagB64,
    };
    final body = utf8.encode(jsonEncode(envelope));
    final meta = jsonEncode({
      'v': 1,
      'created_at_ms': payload['created_at_ms'],
      'size_bytes': plain.length,
    });
    final uri = _relayUri('/_fedi3/backup', {'username': config.username});
    final resp = await http.put(
      uri,
      headers: {
        ..._authHeaders(),
        'Content-Type': 'application/fedi3.backup+json',
        'X-Fedi3-Backup-Meta': meta,
      },
      body: body,
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('backup upload failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<CloudBackupPackage> download() async {
    final uri = _relayUri('/_fedi3/backup/blob', {'username': config.username});
    final resp = await http.get(uri, headers: _authHeaders());
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('backup download failed: ${resp.statusCode} ${resp.body}');
    }
    final envelope = jsonDecode(utf8.decode(resp.bodyBytes)) as Map<String, dynamic>;
    final crypto = BackupCryptoService();
    final plain = crypto.decrypt(
      saltB64: envelope['salt_b64']?.toString() ?? '',
      nonceB64: envelope['nonce_b64']?.toString() ?? '',
      cipherB64: envelope['cipher_b64']?.toString() ?? '',
      tagB64: envelope['tag_b64']?.toString() ?? '',
      token: config.relayToken,
    );
    final payload = jsonDecode(utf8.decode(plain)) as Map<String, dynamic>;
    final cfgJson = (payload['config'] as Map).cast<String, dynamic>();
    final prefsJson = (payload['uiPrefs'] as Map).cast<String, dynamic>();
    final keys = (payload['encryptionKeys'] as Map).cast<String, String>();
    final coreBackup = (payload['coreBackup'] as Map).cast<String, dynamic>();
    return CloudBackupPackage(
      config: CoreConfig.fromJson(cfgJson),
      prefs: UiPrefs.fromJson(prefsJson),
      encryptionKeys: keys,
      coreBackup: coreBackup,
    );
  }

  Future<void> restore(CloudBackupPackage pkg) async {
    final api = CoreApi(config: pkg.config);
    await api.importBackup(pkg.coreBackup);
    await EncryptionManager().importKeys(pkg.encryptionKeys);
  }

  Map<String, String> _authHeaders() => {
        'Authorization': 'Bearer ${config.relayToken}',
      };
}
