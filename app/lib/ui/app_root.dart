/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'package:flutter/material.dart';
import 'dart:ui' show AppExitResponse;
import '../services/app_activity.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

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
  @override
  void initState() {
    super.initState();
    _lifecycle = AppLifecycleListener(
        onStateChange: AppActivity.update,
        onExitRequested: () async {
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
        return MaterialApp(
          onGenerateTitle: (context) => AppLocalizations.of(context)!.appTitle,
          theme: MisskeyTheme.light(
              accent: prefs.accentColor, density: prefs.visualDensity),
          darkTheme: MisskeyTheme.dark(
              accent: prefs.accentColor, density: prefs.visualDensity),
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
