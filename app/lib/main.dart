/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io' show Directory, Platform;

import 'package:path_provider/path_provider.dart';

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:desktop_webview_window/desktop_webview_window.dart';
import 'package:media_kit/media_kit.dart';

import 'state/app_state.dart';
import 'state/config_store.dart';
import 'ui/app_root.dart';
import 'services/notification_service.dart';
import 'services/mobile_push_service.dart';
import 'services/telemetry_service.dart';

Future<void> main(List<String> args) async {
  await runZonedGuarded(() async {
    WidgetsFlutterBinding.ensureInitialized();
    if (kDebugMode) debugPrint('[startup] binding ready');
    final headlessPush =
        Platform.isAndroid && args.contains('--unifiedpush-bg');
    // D6/H-55: media_kit ships libmpv on Linux too, so it must always be
    // initialized. A failure here must not prevent the app from starting.
    try {
      if (!headlessPush) MediaKit.ensureInitialized();
    } catch (_) {
      // Media playback will degrade gracefully; the rest of the app works.
    }
    if (kDebugMode) debugPrint('[startup] media_kit done');
    await NotificationService.init();
    if (kDebugMode) debugPrint('[startup] notifications done');
    // Android 13+ requires a runtime grant before any notification is shown,
    // including the foreground keep-alive service. Ask once at startup instead
    // of waiting for the user to enable a push provider.
    if (!headlessPush && Platform.isAndroid) {
      try {
        await NotificationService.requestPermission();
      } catch (_) {}
    }
    if (!Platform.isLinux && runWebViewTitleBarWidget(args)) return;
    // Android exposes no HOME directory, so both the config store and the Rust
    // core must be pointed at the app support directory explicitly.
    String? coreDataDirBase;
    if (Platform.isAndroid) {
      final support = await getApplicationSupportDirectory();
      final sep = Platform.pathSeparator;
      ConfigStore.useBaseDirectory(Directory('${support.path}${sep}config'));
      coreDataDirBase = '${support.path}${sep}core';
    }
    final appState = await AppState.load(watchdogEnabled: !headlessPush);
    appState.useCoreDataDirBase(coreDataDirBase);
    if (kDebugMode) debugPrint('[startup] app state loaded');
    try {
      await MobilePushService.instance
          .initialize(appState, background: headlessPush);
    } catch (_) {}
    if (headlessPush) return;
    if (kDebugMode) debugPrint('[startup] push service done');
    await TelemetryService.init(() => appState.prefs);
    if (kDebugMode) debugPrint('[startup] telemetry done, runApp');

    FlutterError.onError = (details) {
      FlutterError.presentError(details);
      TelemetryService.record(
        'flutter_error',
        details.exceptionAsString(),
        data: {'stack': details.stack?.toString()},
      );
    };
    PlatformDispatcher.instance.onError = (error, stack) {
      TelemetryService.record(
        'platform_error',
        error.toString(),
        data: {'stack': stack.toString()},
      );
      return true;
    };

    runApp(AppRoot(appState: appState));
  }, (error, stack) {
    TelemetryService.record(
      'zone_error',
      error.toString(),
      data: {'stack': stack.toString()},
      force: kDebugMode,
    );
  });
}
