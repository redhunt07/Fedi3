/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'package:flutter/material.dart';
import 'dart:ui' show AppExitResponse;
import '../services/app_activity.dart';
import '../services/mobile_push_service.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import '../core/fedi3_core.dart' show CorePowerMode;
import '../model/ui_prefs.dart';
import '../state/app_state.dart';
import '../l10n/gen/app_localizations.dart';
import 'screens/first_run_screen.dart';
import 'shell.dart';
import 'theme/misskey_theme.dart';
import 'widgets/auto_start_core.dart';

class AppRoot extends StatefulWidget {
  const AppRoot({super.key, required this.appState});

  final AppState appState;

  @override
  State<AppRoot> createState() => _AppRootState();
}

class _AppRootState extends State<AppRoot> {
  late final AppLifecycleListener _lifecycle;
  AppState get appState => widget.appState;

  /// Theme objects are expensive to build and are re-created on every
  /// AppState notification otherwise (unread counters, prefs writes). Cache
  /// them by the visual identity that actually changes the theme.
  String? _themeKey;
  ThemeData? _lightTheme;
  ThemeData? _darkTheme;

  (ThemeData, ThemeData) _themes(UiPrefs prefs) {
    final key = '${prefs.accent}|${prefs.density}';
    if (key != _themeKey) {
      _themeKey = key;
      _lightTheme =
          MisskeyTheme.light(accent: prefs.accentColor, density: prefs.visualDensity);
      _darkTheme =
          MisskeyTheme.dark(accent: prefs.accentColor, density: prefs.visualDensity);
    }
    return (_lightTheme!, _darkTheme!);
  }
  @override
  void initState() {
    super.initState();
    _lifecycle = AppLifecycleListener(onStateChange: (state) {
      AppActivity.update(state);
      MobilePushService.instance.setForeground(AppActivity.foreground);
      // The native core polls the network on a foreground cadence. Once the
      // UI is no longer visible, stretch it: on a phone the radio is the
      // single largest battery consumer of a backgrounded client.
      appState.setPowerMode(
        AppActivity.foreground
            ? CorePowerMode.foreground
            : CorePowerMode.background,
      );
      // The push-only keep-alive service owns the background: it starts the
      // core on demand and stops it when idle. Leave an active call alone.
      if (AppActivity.foreground) {
        unawaited(appState.startCore());
      } else if (!appState.coreHeld) {
        unawaited(appState.stopCore());
      }
    }, onExitRequested: () async {
      try {
        await appState.stopCore().timeout(const Duration(seconds: 20));
      } catch (_) {}
      return AppExitResponse.exit;
    });
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    AppActivity.update(AppLifecycleState.resumed);
    unawaited(appState.stopCore());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: appState,
      builder: (context, _) {
        final prefs = appState.prefs;
        final (lightTheme, darkTheme) = _themes(prefs);
        return MaterialApp(
          onGenerateTitle: (context) => AppLocalizations.of(context)!.appTitle,
          theme: lightTheme,
          darkTheme: darkTheme,
          themeMode: prefs.flutterThemeMode,
          locale: prefs.flutterLocale,
          localizationsDelegates: const [
            AppLocalizations.delegate,
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          supportedLocales: const [
            Locale('en'),
            Locale('it'),
          ],
          builder: (context, child) {
            final mq = MediaQuery.of(context);
            return MediaQuery(
              data: mq.copyWith(textScaler: TextScaler.linear(prefs.textScale)),
              child: child ?? const SizedBox.shrink(),
            );
          },
          home: appState.config == null
              ? FirstRunScreen(appState: appState)
              : AutoStartCore(
                  appState: appState, child: Shell(appState: appState)),
        );
      },
    );
  }
}
