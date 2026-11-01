/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:fedi3/model/core_config.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/state/config_store.dart';
import 'package:fedi3/state/prefs_store.dart';

import 'fake_core.dart';

/// A valid configuration used by tests (no real network involved).
CoreConfig buildTestConfig({
  String username = 'alice',
  String domain = 'example.test',
  String publicBaseUrl = 'https://example.test',
  String relayWs = 'wss://relay.example.test',
  String relayToken = 'relay-token-0123456789abcdef',
  String bind = '127.0.0.1:8788',
  String internalToken = 'internal-token-0123456789abcdef',
  bool p2pEnable = false,
  String? anchorActorUrl,
}) {
  return CoreConfig(
    username: username,
    domain: domain,
    publicBaseUrl: publicBaseUrl,
    relayWs: relayWs,
    relayToken: relayToken,
    bind: bind,
    internalToken: internalToken,
    p2pEnable: p2pEnable,
    apRelays: const [],
    displayName: 'Alice',
    summary: '',
    iconUrl: '',
    iconMediaType: '',
    imageUrl: '',
    imageMediaType: '',
    profileFields: const [],
    manuallyApprovesFollowers: false,
    blockedDomains: const [],
    blockedActors: const [],
    anchorActorUrl: anchorActorUrl,
  );
}

/// AppState wired to fake FFI and a fake health probe.
AppState buildTestAppState({
  CoreConfig? config,
  UiPrefs? prefs,
  FakeCoreRuntime? core,
  Future<bool> Function(CoreConfig)? healthCheck,
  bool watchdogEnabled = false,
  Duration restartBaseDelay = Duration.zero,
}) {
  return AppState(
    config: config,
    prefs: prefs ?? UiPrefs.defaults(),
    core: core ?? FakeCoreRuntime(),
    healthCheck: healthCheck ?? (_) async => true,
    watchdogEnabled: watchdogEnabled,
    restartBaseDelay: restartBaseDelay,
  );
}

/// Redirects the on-disk stores to a temporary directory.
void useTempStores(Directory dir) {
  ConfigStore.useDirectoryForTesting(dir);
  PrefsStore.useDirectoryForTesting(dir);
}

/// Clears the test overrides.
void resetStoreOverrides() {
  ConfigStore.useDirectoryForTesting(null);
  PrefsStore.useDirectoryForTesting(null);
}
