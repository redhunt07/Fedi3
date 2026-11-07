import 'package:fedi3/state/prefs_store.dart';
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';
import 'dart:async';
import 'package:fedi3/core/fedi3_core.dart';

import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/state/config_store.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/test_config.dart';

class DelayedStopCore extends FakeCoreRuntime implements AsyncCoreStop {
  final release = Completer<void>();
  @override
  Future<void> stopAsync(int handle) async {
    stop(handle);
    await release.future;
  }
}

void main() {
  test('a failed first start still installs an automatic recovery watchdog',
      () async {
    final core =
        FakeCoreRuntime(startError: StateError('port temporarily busy'));
    final app = AppState(
        config: buildTestConfig(),
        prefs: UiPrefs.defaults(),
        core: core,
        watchdogInterval: const Duration(milliseconds: 10),
        restartBaseDelay: Duration.zero,
        healthCheck: (_) async => true);
    try {
      await app.startCore();
      expect(app.isRunning, isFalse);
      core.startError = null;
      for (var tick = 0; tick < 100 && !app.isRunning; tick++) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      expect(app.isRunning, isTrue);
    } finally {
      app.dispose();
    }
  });
  test(
      'dead core handle is stopped and a manual retry can recover after the retry limit',
      () async {
    final core = FakeCoreRuntime(handle: 7);
    final app = buildTestAppState(
        config: buildTestConfig(), core: core, watchdogEnabled: true);
    await app.startCore();
    app.markCoreDead('failed health probe');
    expect(core.stopCalls, 1);
    expect(app.coreHandle, isNull);
    core.startError = StateError('temporarily unavailable');
    for (var attempt = 0; attempt < 8; attempt++) {
      await app.runWatchdogOnce();
    }
    core.startError = null;
    await app.startCore();
    expect(app.isRunning, isTrue);
    app.dispose();
  });
  test('markCoreDead returns immediately and restart waits for native cleanup',
      () async {
    final core = DelayedStopCore();
    final app = buildTestAppState(config: buildTestConfig(), core: core);
    await app.startCore();
    app.markCoreDead('health failed');
    expect(app.isRunning, isFalse);
    expect(app.lastError, 'health failed');
    var timerRan = false;
    await Future<void>.delayed(Duration.zero, () => timerRan = true);
    expect(timerRan, isTrue);
    final restart = app.startCore();
    await Future<void>.delayed(Duration.zero);
    expect(core.startCalls, 1);
    core.release.complete();
    await restart;
    expect(core.startCalls, 2);
    app.dispose();
  });
  test(
      'queued preference mutations preserve concurrent read cursors and UI settings',
      () async {
    final app =
        buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
    final writes = <Future<void>>[];
    for (var i = 0; i < 20; i++) {
      writes.add(app.updatePrefs((current) => current.copyWith(
          chatThreadSeenMs: {'thread-$i': i + 1}, lastChatSeenMs: i + 1)));
    }
    writes.add(app.updatePrefs((current) => current.copyWith(localeTag: 'it')));
    await Future.wait(writes);
    expect(app.prefs.chatThreadSeenMs.length, 20);
    expect(app.prefs.lastChatSeenMs, 20);
    expect(app.prefs.localeTag, 'it');
    final disk = await PrefsStore.read();
    expect(disk.chatThreadSeenMs, app.prefs.chatThreadSeenMs);
    expect(disk.localeTag, 'it');
    app.dispose();
  });
  test('queued preference write can finish after notifier disposal', () async {
    final app =
        buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
    final write = app.updatePrefs((prefs) => prefs.copyWith(localeTag: 'it'));
    app.dispose();
    await write;
    expect((await PrefsStore.read()).localeTag, 'it');
  });
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
