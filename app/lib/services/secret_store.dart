/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import '../model/core_config.dart';
import '../model/ui_prefs.dart';
import '../state/config_store.dart';
import '../state/prefs_store.dart';

class SecretStore {
  SecretStore._();

  static const FlutterSecureStorage _storage = FlutterSecureStorage();

  static const String _migrationKey = 'fedi3.secrets.v1.migrated';

  static const String _relayTokenKey = 'config.relay_token';
  static const String _internalTokenKey = 'config.internal_token';
  static const String _previousRelayTokenKey = 'config.previous_relay_token';

  static const String _relayAdminTokenKey = 'prefs.relay_admin_token';
  static const String _translationAuthKeyKey = 'prefs.translation_auth_key';

  static Future<void> migrateLegacy({
    Map<String, dynamic>? configRaw,
    Map<String, dynamic>? prefsRaw,
  }) async {
    final alreadyMigrated = await _storage.read(key: _migrationKey);
    final hasLegacyConfigSecrets = _hasConfigSecrets(configRaw);
    final hasLegacyPrefsSecrets = _hasPrefSecrets(prefsRaw);
    if (alreadyMigrated == 'true' && !hasLegacyConfigSecrets && !hasLegacyPrefsSecrets) {
      return;
    }

    await _writeIfPresent(_relayTokenKey, _readTrimmed(configRaw, 'relayToken'));
    await _writeIfPresent(_internalTokenKey, _readTrimmed(configRaw, 'internalToken'));
    await _writeIfPresent(
      _previousRelayTokenKey,
      _readTrimmed(configRaw, 'previousRelayToken'),
    );
    await _writeIfPresent(_relayAdminTokenKey, _readTrimmed(prefsRaw, 'relayAdminToken'));
    await _writeIfPresent(
      _translationAuthKeyKey,
      _readTrimmed(prefsRaw, 'translationAuthKey'),
    );

    if (configRaw != null && hasLegacyConfigSecrets) {
      final sanitized = Map<String, dynamic>.from(configRaw)
        ..remove('relayToken')
        ..remove('internalToken')
        ..remove('previousRelayToken');
      ConfigStore.writeConfig(sanitized);
    }
    if (prefsRaw != null && hasLegacyPrefsSecrets) {
      final sanitized = Map<String, dynamic>.from(prefsRaw)
        ..remove('relayAdminToken')
        ..remove('translationAuthKey');
      await PrefsStore.write(UiPrefs.fromJson(sanitized));
    }

    await _storage.write(key: _migrationKey, value: 'true');
  }

  static Future<CoreConfig> hydrateConfig(CoreConfig cfg) async {
    final relayToken = await _storage.read(key: _relayTokenKey) ?? cfg.relayToken;
    final internalToken = await _storage.read(key: _internalTokenKey) ?? cfg.internalToken;
    final previousRelayToken =
        await _storage.read(key: _previousRelayTokenKey) ?? cfg.previousRelayToken;
    return cfg.copyWith(
      relayToken: relayToken,
      internalToken: internalToken,
      previousRelayToken: previousRelayToken,
    );
  }

  static Future<UiPrefs> hydratePrefs(UiPrefs prefs) async {
    final relayAdminToken =
        await _storage.read(key: _relayAdminTokenKey) ?? prefs.relayAdminToken;
    final translationAuthKey =
        await _storage.read(key: _translationAuthKeyKey) ?? prefs.translationAuthKey;
    return prefs.copyWith(
      relayAdminToken: relayAdminToken,
      translationAuthKey: translationAuthKey,
    );
  }

  static Future<void> saveConfigSecrets(CoreConfig cfg) async {
    await _writeIfPresent(_relayTokenKey, cfg.relayToken);
    await _writeIfPresent(_internalTokenKey, cfg.internalToken);
    await _writeIfPresent(_previousRelayTokenKey, cfg.previousRelayToken);
    await _storage.write(key: _migrationKey, value: 'true');
  }

  static Future<void> savePrefsSecrets(UiPrefs prefs) async {
    await _writeIfPresent(_relayAdminTokenKey, prefs.relayAdminToken);
    await _writeIfPresent(_translationAuthKeyKey, prefs.translationAuthKey);
    await _storage.write(key: _migrationKey, value: 'true');
  }

  static Future<void> clearConfigSecrets() async {
    await _storage.delete(key: _relayTokenKey);
    await _storage.delete(key: _internalTokenKey);
    await _storage.delete(key: _previousRelayTokenKey);
  }

  static Future<void> clearPrefsSecrets() async {
    await _storage.delete(key: _relayAdminTokenKey);
    await _storage.delete(key: _translationAuthKeyKey);
  }

  static bool _hasConfigSecrets(Map<String, dynamic>? raw) {
    if (raw == null) return false;
    return _readTrimmed(raw, 'relayToken').isNotEmpty ||
        _readTrimmed(raw, 'internalToken').isNotEmpty ||
        _readTrimmed(raw, 'previousRelayToken').isNotEmpty;
  }

  static bool _hasPrefSecrets(Map<String, dynamic>? raw) {
    if (raw == null) return false;
    return _readTrimmed(raw, 'relayAdminToken').isNotEmpty ||
        _readTrimmed(raw, 'translationAuthKey').isNotEmpty;
  }

  static String _readTrimmed(Map<String, dynamic>? raw, String key) {
    return raw?[key]?.toString().trim() ?? '';
  }

  static Future<void> _writeIfPresent(String key, String? value) async {
    final trimmed = value?.trim() ?? '';
    if (trimmed.isEmpty) {
      await _storage.delete(key: key);
      return;
    }
    await _storage.write(key: key, value: trimmed);
  }
}
