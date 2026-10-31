/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/state/config_store.dart';
import 'package:fedi3/state/prefs_store.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/test_config.dart';

void main() {
  late Directory tempDir;

  setUp(() async {
    tempDir = await Directory.systemTemp.createTemp('fedi3_store_test_');
    useTempStores(tempDir);
  });

  tearDown(() async {
    resetStoreOverrides();
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });

  group('ConfigStore (AS-01..04)', () {
    test('AS-01 write is atomic: no temp file remains and JSON parses', () {
      ConfigStore.writeConfig({'username': 'alice', 'domain': 'example.test'});
      final file = ConfigStore.configFile();
      expect(file.existsSync(), isTrue);
      expect(File('${file.path}.tmp').existsSync(), isFalse);
      final decoded = jsonDecode(file.readAsStringSync());
      expect(decoded, isA<Map>());
      expect((decoded as Map)['username'], 'alice');

      // A second write keeps the previous content as a valid backup.
      ConfigStore.writeConfig({'username': 'bob', 'domain': 'example.test'});
      final bak = ConfigStore.backupFile();
      expect(bak.existsSync(), isTrue);
      expect(jsonDecode(bak.readAsStringSync()),
          containsPair('username', 'alice'));
      expect(ConfigStore.load().status, ConfigLoadStatus.ok);
    });

    test('AS-02 corrupt config is recovered from a valid .bak', () {
      ConfigStore.writeConfig({'username': 'alice', 'domain': 'example.test'});
      ConfigStore.writeConfig({'username': 'bob', 'domain': 'example.test'});
      ConfigStore.configFile().writeAsStringSync('{"broken": ');

      final result = ConfigStore.load();
      expect(result.status, ConfigLoadStatus.recoveredFromBackup);
      expect(result.data?['username'], 'alice');
      // The main file is restored as well.
      expect(ConfigStore.load().status, ConfigLoadStatus.ok);
    });

    test('AS-03 corrupt config without backup is distinct from absent', () {
      expect(ConfigStore.load().status, ConfigLoadStatus.absent);
      ConfigStore.configFile().writeAsStringSync('not json at all');
      final result = ConfigStore.load();
      expect(result.status, ConfigLoadStatus.corrupt);
      expect(result.isCorrupt, isTrue);
      expect(result.data, isNull);
    });

    test('AS-04 config file has 0600 permissions on POSIX', () {
      if (Platform.isWindows) return;
      ConfigStore.writeConfig({'username': 'alice'});
      final mode = ConfigStore.configFile().statSync().mode & 0x1FF;
      expect(mode, 0x180); // 0600
    });
  });

  group('PrefsStore (AS-05)', () {
    test('AS-05 corrupt prefs fall back to defaults with a corrupt flag',
        () async {
      final file =
          File('${tempDir.path}${Platform.pathSeparator}ui_prefs.json');
      await file.writeAsString('{oops');
      final result = await PrefsStore.load();
      expect(result.corrupt, isTrue);
      expect(result.prefs.toJson(), equals(UiPrefs.defaults().toJson()));
    });

    test('PrefsStore write/read round-trips and keeps a backup', () async {
      final prefs = UiPrefs.defaults().copyWith(themeMode: UiThemeMode.dark);
      await PrefsStore.write(prefs);
      final result = await PrefsStore.load();
      expect(result.corrupt, isFalse);
      expect(result.prefs.themeMode, UiThemeMode.dark);

      await PrefsStore.write(UiPrefs.defaults());
      final bak =
          File('${tempDir.path}${Platform.pathSeparator}ui_prefs.json.bak');
      expect(await bak.exists(), isTrue);
    });
  });
}
