/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// This is a basic Flutter widget test.
//
// To perform an interaction with a widget in your test, use the WidgetTester
// utility in the flutter_test package. For example, you can send tap and scroll
// gestures. You can also use WidgetTester to find child widgets in the widget
// tree, read text, and verify that the values of widget properties are correct.

import 'package:flutter_test/flutter_test.dart';

import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/ui/app_root.dart';
import 'package:fedi3/ui/screens/first_run_screen.dart';

void main() {
  testWidgets('App boots', (WidgetTester tester) async {
    final appState = AppState(config: null, prefs: UiPrefs.defaults());
    await tester.pumpWidget(AppRoot(appState: appState));
    expect(find.byType(FirstRunScreen), findsOneWidget);
  });
}
