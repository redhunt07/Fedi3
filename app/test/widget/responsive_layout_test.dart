/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/theme/misskey_theme.dart';
import 'package:fedi3/ui/screens/first_run_screen.dart';
import 'package:fedi3/ui/screens/onboarding_screen.dart';
import 'package:fedi3/ui/screens/settings_screen.dart';
import 'package:fedi3/ui/screens/pairing_screen.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  for (final width in [320.0, 390.0, 1280.0]) {
    for (final locale in [const Locale('it'), const Locale('en')]) {
      for (final dark in [false, true]) {
        testWidgets(
          'entry and settings fit $width / $locale / dark=$dark at 2x text',
          (tester) async {
            final dir = await setUpPumpEnv(tester);
            await tester.binding.setSurfaceSize(Size(width, 900));
            addTearDown(() => tester.binding.setSurfaceSize(null));
            CoreHttp.useTestClient(
              FakeHttpClient(defaultResponse: FakeHttpResponse(body: '{}')),
            );
            final app = buildTestAppState(config: buildTestConfig());
            try {
              for (final screen in <Widget>[
                FirstRunScreen(appState: app),
                OnboardingScreen(appState: app, mode: OnboardingMode.createNew),
                SettingsScreen(appState: app),
                PairingScreen(appState: app),
              ]) {
                await pumpScreen(
                  tester,
                  screen,
                  locale: locale,
                  textScale: 2,
                  theme: dark ? MisskeyTheme.dark() : MisskeyTheme.light(),
                );
                await tester.pumpAndSettle();
                expect(
                  tester.takeException(),
                  isNull,
                  reason: '${screen.runtimeType} at width $width ($locale)',
                );
                final scrollables = find.byType(Scrollable);
                if (scrollables.evaluate().isNotEmpty) {
                  await tester.drag(scrollables.first, const Offset(0, -1400));
                  await tester.pumpAndSettle();
                  expect(
                    tester.takeException(),
                    isNull,
                    reason: 'scrolled ${screen.runtimeType}',
                  );
                }
                await tester.pumpWidget(const SizedBox.shrink());
              }
            } finally {
              app.dispose();
              CoreHttp.reset();
              resetStoreOverrides();
              dir.deleteSync(recursive: true);
            }
          },
        );
      }
    }
  }
}
