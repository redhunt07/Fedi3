/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import '../state/app_state.dart';
import '../l10n/gen/app_localizations.dart';
import 'screens/first_run_screen.dart';
import 'shell.dart';
import 'theme/misskey_theme.dart';
import 'widgets/auto_start_core.dart';

class AppRoot extends StatelessWidget {
  const AppRoot({super.key, required this.appState});

  final AppState appState;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: appState,
      builder: (context, _) {
        final prefs = appState.prefs;
        return MaterialApp(
          onGenerateTitle: (context) => AppLocalizations.of(context)!.appTitle,
          theme: MisskeyTheme.light(accent: prefs.accentColor, density: prefs.visualDensity),
          darkTheme: MisskeyTheme.dark(accent: prefs.accentColor, density: prefs.visualDensity),
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
              : AutoStartCore(appState: appState, child: Shell(appState: appState)),
        );
      },
    );
  }
}
