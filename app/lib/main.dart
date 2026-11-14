/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io' show Directory, Platform;
import 'dart:ui' show DartPluginRegistrant, PlatformDispatcher;

import 'package:path_provider/path_provider.dart';

import 'package:material_ui/material_ui.dart';
import 'package:flutter/foundation.dart';
import 'package:desktop_webview_window/desktop_webview_window.dart';
import 'package:media_kit/media_kit.dart';

import 'state/app_state.dart';
import 'state/config_store.dart';
import 'ui/app_root.dart';
import 'core/core_api.dart';
import 'core/fedi3_core.dart' show CorePowerMode;
import 'l10n/gen/app_localizations.dart';
import 'services/core_event_stream.dart';
import 'services/call_activity_marker.dart';
import 'services/incoming_call_notifications.dart';
import 'services/notification_service.dart';
import 'services/relay_spool_poller.dart';
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

/// Entry point for the Android keep-alive service engine.
///
/// Push-only mode: the service does not keep the Rust core or the P2P swarm
/// alive while the phone is idle. It polls the relay's chat spool over HTTPS;
/// when new items exist it starts the core, lets it sync and show the real
/// notifications (chat previews, incoming calls, activity), then stops it
/// again after a quiet period. Accounts without a relay (P2P-only) keep the
/// core running, because there is no relay spool to poll.
@pragma('vm:entry-point')
Future<void> backgroundMain() async {
  WidgetsFlutterBinding.ensureInitialized();
  DartPluginRegistrant.ensureInitialized();
  await NotificationService.init();
  if (!Platform.isAndroid) return;
  final support = await getApplicationSupportDirectory();
  final sep = Platform.pathSeparator;
  ConfigStore.useBaseDirectory(Directory('${support.path}${sep}config'));
  final appState = await AppState.load(watchdogEnabled: false);
  appState.useCoreDataDirBase('${support.path}${sep}core');
  final cfg = appState.config;
  debugPrint('[bg] cfg=${cfg != null}');
  if (cfg == null) return;
  final l10n = lookupAppLocalizations(PlatformDispatcher.instance.locale);
  final lastChatSeen = appState.prefs.lastChatSeenMs;

  StreamSubscription<CoreEvent>? events;
  IncomingCallNotifications? calls;
  Timer? quiet;
  Timer? hardStop;
  var coreUp = false;
  var starting = false;

  late final void Function() armQuiet;
  late final void Function() armHardStop;
  late final Future<void> Function({bool force}) stopBackgroundCore;

  armQuiet = () {
    quiet?.cancel();
    quiet = Timer(const Duration(seconds: 30), () {
      debugPrint('[bg] quiet fired');
      unawaited(stopBackgroundCore());
    });
  };

  // A wake may keep receiving events (one per synced item), so the quiet timer
  // alone can be re-armed forever. Cap how long a push-only core may stay up;
  // an active call re-arms the cap while its heartbeat stays fresh.
  armHardStop = () {
    hardStop?.cancel();
    hardStop = Timer(const Duration(seconds: 120), () {
      debugPrint('[bg] hard stop fired');
      unawaited(stopBackgroundCore(force: true));
    });
  };

  stopBackgroundCore = ({bool force = false}) async {
    quiet?.cancel();
    quiet = null;
    // The UI isolate owns the call screen; its hold flag is not visible here.
    // The heartbeat file is: while a call is active keep the core (signalling
    // runs in it) and check again after another quiet period.
    if (await CallActivityMarker.isActive()) {
      debugPrint('[bg] call active, keeping core');
      armQuiet();
      if (force) armHardStop();
      return;
    }
    // A cancelled SSE subscription can take a while to unwind; the core must
    // not wait for it forever.
    try {
      await (events?.cancel() ?? Future<void>.value())
          .timeout(const Duration(seconds: 3));
    } catch (_) {}
    events = null;
    calls?.stop();
    calls = null;
    hardStop?.cancel();
    hardStop = null;
    if (coreUp) {
      coreUp = false;
      try {
        await appState.stopCore().timeout(const Duration(seconds: 25));
      } catch (e) {
        debugPrint('[bg] core stop error: $e');
      }
      debugPrint('[bg] core stopped (idle)');
    }
  };

  Future<void> startBackgroundCore({bool pushOnly = true}) async {
    if (starting) throw StateError("Core startup pending");
    starting = true;
    try {
      final healthy = appState.isRunning && await appState.probeCoreHealth();
      if (!healthy) {
        debugPrint('[bg] starting core for wake');
        await appState.startCore();
      }
      if (!await appState.probeCoreHealth()) {
        throw StateError("Background core did not start");
      }
      appState.setPowerMode(CorePowerMode.background);
      coreUp = true;
      calls ??= IncomingCallNotifications(
        api: CoreApi(config: cfg),
        selfActor:
            '${cfg.publicBaseUrl.replaceFirst(RegExp(r'/$'), '')}/users/${cfg.username}',
        onInvite: (signal) => NotificationService.showCallNotification(
          title: l10n.callIncoming,
          body: signal.threadTitle.isNotEmpty
              ? signal.threadTitle
              : signal.fromActor.split('/').last,
          threadId: signal.threadId,
        ),
        onEnded: NotificationService.cancelCallNotification,
      )..start();
      events ??= CoreEventStream(config: cfg)
          .stream(background: true, reconnect: true)
          .listen((ev) async {
        if (pushOnly) armQuiet();
        if (ev.kind == 'notification' || ev.kind == 'inbox') {
          await NotificationService.showActivityNotification(
            title: l10n.activityNotificationTitle,
            body: l10n.activityNotificationBody,
          );
        } else if (ev.kind == 'chat' && ev.activityType == 'call') {
          await calls?.poll();
        } else if (ev.kind == 'chat' &&
            (ev.activityType == 'message' || ev.activityType == 'system') &&
            ev.tsMs > lastChatSeen) {
          var body = l10n.chatNewMessageBody;
          try {
            final threads =
                await CoreApi(config: cfg).fetchChatThreads(limit: 20);
            final items = (threads['items'] as List?) ?? const <dynamic>[];
            for (final item in items.whereType<Map<String, dynamic>>()) {
              if (item['thread_id'] != ev.activityId) continue;
              final peers =
                  (item['peer_usernames'] as List?)?.cast<String>() ?? const [];
              final preview = (item['last_message_preview'] as String?) ?? '';
              final title = (item['title'] as String?) ?? '';
              final who = title.isNotEmpty
                  ? title
                  : (peers.isNotEmpty ? '@${peers.first}' : '');
              if (who.isNotEmpty || preview.isNotEmpty) {
                body = who.isEmpty ? preview : '$who: $preview';
              }
              break;
            }
          } catch (_) {
            // Keep the generic body when the thread list is unavailable.
          }
          await NotificationService.showChatNotification(
            title: l10n.chatTitle,
            body: body,
            threadId: ev.activityId,
          );
        }
      }, onError: (_) {
        unawaited(stopBackgroundCore());
      });
      if (pushOnly) {
        armQuiet();
        armHardStop();
      }
    } catch (e) {
      debugPrint('[bg] core start failed: $e');
      coreUp = false;
      rethrow;
    } finally {
      starting = false;
    }
  }

  final poller = RelaySpoolPoller(
    config: cfg,
    directory: support,
    onNewItems: startBackgroundCore,
  );
  if (poller.enabled) {
    await poller.start();
    debugPrint('[bg] push-only spool poller started');
    // Adopt a core left running by a previous engine, a call hold or a race
    // with the UI's stop: the quiet timer drains it and stops it. Without this
    // the orphan core would keep the P2P swarm and the relay tunnel alive.
    if (await appState.probeCoreHealth()) {
      debugPrint('[bg] adopting running core');
      await startBackgroundCore();
    }
    // Safety net for cores that appear later (for example a UI engine that
    // died without stopping its core): one loopback probe per minute.
    Timer.periodic(const Duration(seconds: 60), (_) {
      unawaited(() async {
        if (coreUp || starting) return;
        if (await appState.probeCoreHealth()) {
          debugPrint('[bg] adopting running core');
          await startBackgroundCore();
        }
      }());
    });
  } else {
    // P2P-only account: no relay spool to poll, keep the core running.
    await startBackgroundCore(pushOnly: false);
    debugPrint('[bg] P2P-only core started');
  }
}
