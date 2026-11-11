/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:fedi3/l10n/gen/app_localizations.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:cupertino_ui/cupertino_ui.dart';
import 'package:flutter_localizations/flutter_localizations.dart'
    hide GlobalMaterialLocalizations, GlobalCupertinoLocalizations;
import 'package:flutter_test/flutter_test.dart';

import 'test_config.dart';

/// Mocks `path_provider` so stores that use the application support directory
/// never touch the real filesystem.
Future<void> mockPathProvider(WidgetTester tester, Directory dir) async {
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  const channels = [
    'plugins.flutter.io/path_provider',
    'plugins.flutter.io/path_provider_android',
    'plugins.flutter.io/path_provider_linux',
    'plugins.flutter.io/path_provider_foundation',
    'plugins.flutter.io/path_provider_windows',
  ];
  for (final name in channels) {
    messenger.setMockMethodCallHandler(
      MethodChannel(name),
      (call) async => dir.path,
    );
  }
  useTempStores(dir);
}

/// Prepares a temp directory + store overrides for a widget test.
Future<Directory> setUpPumpEnv(WidgetTester tester) async {
  final dir = Directory.systemTemp.createTempSync('fedi3_widget_test_');
  await mockPathProvider(tester, dir);
  return dir;
}

/// Mounts [child] inside a localized MaterialApp, with an optional text scale.
Future<void> pumpScreen(
  WidgetTester tester,
  Widget child, {
  Locale locale = const Locale('en'),
  double textScale = 1.0,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      locale: locale,
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: const [Locale('en'), Locale('it')],
      builder: (context, appChild) {
        final mq = MediaQuery.of(context);
        return MediaQuery(
          data: mq.copyWith(textScaler: TextScaler.linear(textScale)),
          child: appChild ?? const SizedBox.shrink(),
        );
      },
      home: child,
    ),
  );
  await tester.pump();
}
