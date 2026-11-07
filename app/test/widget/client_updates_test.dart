/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/client_updates_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';

import '../support/fake_http.dart';
import '../support/pump_app.dart';

void main() {
  late FakeHttpClient client;
  setUp(() {
    PackageInfo.setMockInitialValues(
        appName: 'Fedi3',
        packageName: 'fedi3',
        version: '0.10.0',
        buildNumber: '1',
        buildSignature: '');
    client = FakeHttpClient();
    CoreHttp.useTestClient(client);
  });
  tearDown(CoreHttp.reset);

  void release(String version) {
    client.json('GET', '/repos/redhunt07/Fedi3/releases/latest', {
      'draft': false,
      'prerelease': false,
      'tag_name': 'v$version',
      'html_url': 'https://github.com/redhunt07/Fedi3/releases/tag/v$version',
      'assets': <dynamic>[],
      'body': 'New release notes',
    });
  }

  testWidgets(
      'shows newer release and manual installation when no compatible asset exists',
      (tester) async {
    release('0.10.1');
    await pumpScreen(tester, const ClientUpdatesScreen());
    await tester.pumpAndSettle();
    expect(find.text('Version 0.10.1 is available'), findsOneWidget);
    expect(find.text('Installed version: 0.10.0'), findsOneWidget);
    expect(find.text('View release'), findsOneWidget);
    expect(find.text('Download verified update'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('does not offer an older release as an update', (tester) async {
    release('0.9.19');
    await pumpScreen(tester, const ClientUpdatesScreen());
    await tester.pumpAndSettle();
    expect(find.text('No newer stable release is available.'), findsOneWidget);
    expect(find.text('View release'), findsNothing);
  });

  testWidgets('manual retry recovers from rate limiting', (tester) async {
    client.route('GET', '/repos/redhunt07/Fedi3/releases/latest',
        FakeHttpResponse(statusCode: 429));
    await pumpScreen(tester, const ClientUpdatesScreen());
    await tester.pumpAndSettle();
    expect(find.textContaining('HTTP 429'), findsOneWidget);
    release('0.10.1');
    await tester.tap(find.text('Check for updates'));
    await tester.pumpAndSettle();
    expect(find.text('Version 0.10.1 is available'), findsOneWidget);
    expect(find.textContaining('HTTP 429'), findsNothing);
  });

  testWidgets('Italian narrow layout has no overflow', (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(360, 740);
    addTearDown(() {
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
    });
    release('0.10.1');
    await pumpScreen(tester, const ClientUpdatesScreen(),
        locale: const Locale('it'));
    await tester.pumpAndSettle();
    expect(find.text('È disponibile la versione 0.10.1'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
