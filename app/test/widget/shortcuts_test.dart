/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/ui/screens/search_screen.dart';
import 'package:fedi3/ui/screens/timelines_screen.dart';
import 'package:fedi3/ui/shell.dart';
import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

Finder _tooltip(String message) => find.byWidgetPredicate(
      (widget) => widget is Tooltip && widget.message == message,
    );

Future<AppState> _runningApp() async {
  final app =
      buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
  await app.startCore();
  return app;
}

void main() {
  testWidgets('AW-40 Ctrl+2 opens Search and Ctrl+1 returns to Timeline',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(FakeHttpClient(
        defaultResponse: FakeHttpResponse(body: '{"items":[],"next":null}')));
    addTearDown(CoreHttp.reset);

    await tester.binding.setSurfaceSize(const Size(1400, 900));
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final app = await _runningApp();
    await pumpScreen(tester, Shell(appState: app));
    await tester.pump();
    await tester.pump();
    expect(find.byType(TimelinesScreen), findsOneWidget);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit2);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(find.byType(SearchScreen), findsOneWidget);
    expect(find.byType(TimelinesScreen), findsNothing);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit1);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(find.byType(TimelinesScreen), findsOneWidget);
    expect(find.byType(SearchScreen), findsNothing);

    debugDefaultTargetPlatformOverride = null;
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 3));
  });

  testWidgets(
      'AW-41 desktop rail items expose accessible labels with shortcut hints',
      (tester) async {
    final semantics = tester.ensureSemantics();
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(FakeHttpClient(
        defaultResponse: FakeHttpResponse(body: '{"items":[],"next":null}')));
    addTearDown(CoreHttp.reset);

    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1400, 900);
    addTearDown(() {
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
    });
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    final app = await _runningApp();
    await pumpScreen(tester, Shell(appState: app));
    await tester.pump();
    await tester.pump();

    expect(_tooltip('Timeline (Ctrl+1)'), findsWidgets);
    expect(_tooltip('Search (Ctrl+2)'), findsWidgets);
    expect(_tooltip('Relays (Ctrl+5)'), findsWidgets);
    expect(_tooltip('Settings (Ctrl+6)'), findsWidgets);
    expect(find.byType(NavigationBar), findsNothing);

    semantics.dispose();
    debugDefaultTargetPlatformOverride = null;
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 3));
  });

  testWidgets('AW-41b phone bottom bar keeps five readable sections',
      (tester) async {
    await setUpPumpEnv(tester);
    CoreHttp.useTestClient(FakeHttpClient(
        defaultResponse: FakeHttpResponse(body: '{"items":[],"next":null}')));
    addTearDown(CoreHttp.reset);

    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(400, 900);
    addTearDown(() {
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
    });
    final app = await _runningApp();
    await pumpScreen(tester, Shell(appState: app));
    await tester.pump();
    await tester.pump();

    expect(find.byType(NavigationBar), findsOneWidget);
    expect(_tooltip('Timeline (Ctrl+1)'), findsWidgets);
    expect(_tooltip('Search (Ctrl+2)'), findsWidgets);
    expect(_tooltip('Chat (Ctrl+3)'), findsWidgets);
    expect(_tooltip('Notifications (Ctrl+4)'), findsWidgets);
    expect(_tooltip('Account (Ctrl+7)'), findsWidgets);
    // Relay management and settings are reachable from the account tab, so the
    // phone bar must not crowd them in.
    expect(_tooltip('Relays (Ctrl+5)'), findsNothing);
    expect(_tooltip('Settings (Ctrl+6)'), findsNothing);
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 3));
  });
}
