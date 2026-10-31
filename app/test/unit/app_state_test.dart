/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/state/config_store.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/test_config.dart';

void main() {
  late Directory tempDir;

  setUp(() async {
    tempDir = await Directory.systemTemp.createTemp('fedi3_appstate_test_');
    useTempStores(tempDir);
  });

  tearDown(() async {
    resetStoreOverrides();
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });

  group('core lifecycle (AP-01..03, AP-06)', () {
    test('AP-01 startCore stores the handle only after FFI returns', () async {
      final core = FakeCoreRuntime(handle: 7);
      final app = buildTestAppState(config: buildTestConfig(), core: core);
      await app.startCore();
      expect(core.startCalls, 1);
      expect(app.coreHandle, 7);
      expect(app.isRunning, isTrue);
      expect(app.lastError, isNull);
      final startJson =
          jsonDecode(core.lastConfigJson!) as Map<String, dynamic>;
      expect(startJson['username'], 'alice');
    });

    test('AP-02 FFI failure leaves the handle null and records the error',
        () async {
      final core = FakeCoreRuntime(startError: StateError('bind failed'));
      final app = buildTestAppState(config: buildTestConfig(), core: core);
      await app.startCore();
      expect(core.startCalls, 1);
      expect(app.coreHandle, isNull);
      expect(app.isRunning, isFalse);
      expect(app.lastError, contains('bind failed'));
    });

    test('AP-03 stopCore clears the handle and calls FFI stop', () async {
      final core = FakeCoreRuntime(handle: 9);
      final app = buildTestAppState(config: buildTestConfig(), core: core);
      await app.startCore();
      await app.stopCore();
      expect(app.coreHandle, isNull);
      expect(app.isRunning, isFalse);
      expect(core.stopCalls, 1);
      expect(core.stoppedHandles, [9]);
    });

    test('AP-06 concurrent startCore issues a single FFI call', () async {
      final core = FakeCoreRuntime();
      final app = buildTestAppState(config: buildTestConfig(), core: core);
      final first = app.startCore();
      final second = app.startCore();
      await Future.wait([first, second]);
      expect(core.startCalls, 1);
      expect(app.isRunning, isTrue);
    });
  });

  group('watchdog (AP-04)', () {
    test('AP-04 health failure marks the core dead and attempts a restart',
        () async {
      final core = FakeCoreRuntime(handle: 3);
      var healthCalls = 0;
      final app = buildTestAppState(
        config: buildTestConfig(),
        core: core,
        healthCheck: (_) async {
          healthCalls++;
          return false;
        },
        watchdogEnabled: true,
      );
      await app.startCore();
      expect(app.isRunning, isTrue);
      await app.runWatchdogOnce();
      expect(healthCalls, 1);
      expect(core.startCalls, 2, reason: 'watchdog must restart the dead core');
      expect(app.isRunning, isTrue);
      expect(app.lastError, isNull);
    });

    test('watchdog stops retrying after max attempts', () async {
      final core = FakeCoreRuntime(startError: StateError('still dead'));
      final app = buildTestAppState(
        config: buildTestConfig(),
        core: core,
        healthCheck: (_) async => false,
        watchdogEnabled: true,
      );
      await app.startCore();
      for (var i = 0; i < 8; i++) {
        await app.runWatchdogOnce();
      }
      // 1 initial start + at most restartMaxAttempts (5) retries.
      expect(app.isRunning, isFalse);
      expect(core.startCalls, lessThanOrEqualTo(6));
      final attempts = core.startCalls;
      await app.runWatchdogOnce();
      expect(core.startCalls, attempts,
          reason: 'no further retries after the cap');
    });

    test('healthy core is not restarted', () async {
      final core = FakeCoreRuntime();
      final app = buildTestAppState(
        config: buildTestConfig(),
        core: core,
        healthCheck: (_) async => true,
        watchdogEnabled: true,
      );
      await app.startCore();
      await app.runWatchdogOnce();
      expect(core.startCalls, 1);
      expect(app.isRunning, isTrue);
    });
  });

  group('replaceConfig (AP-05)', () {
    test('AP-05 replaceConfig performs stop -> save -> start', () async {
      final core = FakeCoreRuntime();
      final app = buildTestAppState(config: buildTestConfig(), core: core);
      await app.startCore();
      final next = buildTestConfig(username: 'bob');
      final ok = await app.replaceConfig(next);
      expect(ok, isTrue);
      expect(core.stopCalls, 1);
      expect(core.startCalls, 2);
      expect(app.isRunning, isTrue);
      final saved = ConfigStore.load().data!;
      expect(saved['username'], 'bob');
    });

    test('replaceConfig restores the previous config when the new one fails',
        () async {
      final core = FakeCoreRuntime();
      final app = buildTestAppState(config: buildTestConfig(), core: core);
      await app.startCore();
      core.startError = StateError('new config rejected');
      await expectLater(
        app.replaceConfig(buildTestConfig(username: 'bob')),
        throwsA(isA<StateError>()),
      );
      expect(app.config?.username, 'alice');
      expect(ConfigStore.load().data!['username'], 'alice');
      expect(app.isRunning, isFalse);
    });

    test('replaceConfig keeps the core running after a config edit', () async {
      final core = FakeCoreRuntime();
      final app = buildTestAppState(config: buildTestConfig(), core: core);
      await app.startCore();
      await app.replaceConfig(buildTestConfig(bind: '127.0.0.1:9999'));
      expect(app.isRunning, isTrue);
      final startJson =
          jsonDecode(core.lastConfigJson!) as Map<String, dynamic>;
      expect(startJson['bind'], '127.0.0.1:9999');
    });
  });

  group('load', () {
    test('load distinguishes corrupt config from absent', () async {
      ConfigStore.configFile().writeAsStringSync('{broken');
      final app =
          await AppState.load(core: FakeCoreRuntime(), watchdogEnabled: false);
      expect(app.config, isNull);
      expect(app.configCorrupt, isTrue);
      expect(app.lastError, isNotNull);
    });

    test('load recovers a config from backup', () async {
      ConfigStore.writeConfig({'username': 'alice'});
      ConfigStore.writeConfig({'username': 'bob'});
      ConfigStore.configFile().writeAsStringSync('{broken');
      final app =
          await AppState.load(core: FakeCoreRuntime(), watchdogEnabled: false);
      expect(app.config?.username, 'alice');
      expect(app.configCorrupt, isFalse);
    });
  });
}
