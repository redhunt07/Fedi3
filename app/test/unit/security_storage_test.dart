import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:fedi3/services/backup_codec.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/state/config_store.dart';
import '../support/test_config.dart';

void main() {
  test('encrypted settings authenticate data and preserve legacy imports',
      () async {
    final config = buildTestConfig();
    final prefs = UiPrefs.defaults();
    final raw = await BackupCodec.encodeEncrypted(
        config: config, prefs: prefs, passphrase: 'a long private passphrase');
    expect(raw, isNot(contains(config.relayToken)));
    expect(
        (await BackupCodec.decodeEncrypted(raw,
                passphrase: 'a long private passphrase'))
            .config
            .relayToken,
        config.relayToken);
    await expectLater(
        BackupCodec.decodeEncrypted(raw,
            passphrase: 'a wrong private passphrase'),
        throwsFormatException);
    final changed = jsonDecode(raw) as Map<String, dynamic>;
    final ciphertext = base64Decode(changed['ciphertext'] as String);
    ciphertext[0] ^= 1;
    changed['ciphertext'] = base64Encode(ciphertext);
    await expectLater(
        BackupCodec.decodeEncrypted(jsonEncode(changed),
            passphrase: 'a long private passphrase'),
        throwsFormatException);
    final legacy = BackupCodec.encode(config: config, prefs: prefs);
    expect(
        (await BackupCodec.decodeEncrypted(legacy, passphrase: ''))
            .config
            .username,
        config.username);
  });

  test('legacy config and backup migrate without secrets on disk', () async {
    final dir = await Directory.systemTemp.createTemp('fedi3-keyring-test');
    ConfigStore.useDirectoryForTesting(dir);
    try {
      final config = buildTestConfig().toJson();
      ConfigStore.writeConfig(config);
      ConfigStore.writeConfig({...config, 'username': 'bob'});
      final result = await ConfigStore.loadSecure();
      expect(result.data!['relayToken'], config['relayToken']);
      for (final file in [ConfigStore.configFile(), ConfigStore.backupFile()]) {
        expect(file.readAsStringSync(),
            isNot(contains(config['relayToken'] as String)));
        expect(jsonDecode(file.readAsStringSync()), contains('_secretRef'));
      }
      ConfigStore.configFile().writeAsStringSync('{broken');
      final recovered = await ConfigStore.loadSecure();
      expect(recovered.status, ConfigLoadStatus.recoveredFromBackup);
      expect(recovered.data!['internalToken'], config['internalToken']);
    } finally {
      ConfigStore.useDirectoryForTesting(null);
      await dir.delete(recursive: true);
    }
  });
  test('missing keyring record preserves config and prevents autostart',
      () async {
    final dir = await Directory.systemTemp.createTemp('fedi3-locked-keyring');
    ConfigStore.useDirectoryForTesting(dir);
    try {
      const raw =
          '{"username":"alice","_secretRef":"fedi3.config.unavailable"}';
      ConfigStore.configFile().writeAsStringSync(raw);
      final result = await ConfigStore.loadSecure();
      expect(result.isUnavailable, isTrue);
      expect(result.isCorrupt, isFalse);
      expect(result.data, isNull);
      expect(ConfigStore.configFile().readAsStringSync(), raw);
    } finally {
      ConfigStore.useDirectoryForTesting(null);
      await dir.delete(recursive: true);
    }
  });
}
