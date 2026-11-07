/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io' show Platform;

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:desktop_webview_window/desktop_webview_window.dart';
import 'package:media_kit/media_kit.dart';

import 'state/app_state.dart';
import 'ui/app_root.dart';
import 'services/notification_service.dart';
import 'services/mobile_push_service.dart';
import 'services/telemetry_service.dart';

Future<void> main(List<String> args) async {
  await runZonedGuarded(() async {
    WidgetsFlutterBinding.ensureInitialized();
    final headlessPush =
        Platform.isAndroid && args.contains('--unifiedpush-bg');
    // D6/H-55: media_kit ships libmpv on Linux too, so it must always be
    // initialized. A failure here must not prevent the app from starting.
    try {
      if (!headlessPush) MediaKit.ensureInitialized();
    } catch (_) {
      // Media playback will degrade gracefully; the rest of the app works.
    }
    await NotificationService.init();
    if (!Platform.isLinux && runWebViewTitleBarWidget(args)) return;
    final appState = await AppState.load(watchdogEnabled: !headlessPush);
    try {
      await MobilePushService.instance
          .initialize(appState, background: headlessPush);
    } catch (_) {}
    if (headlessPush) return;
    await TelemetryService.init(() => appState.prefs);

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
