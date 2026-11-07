/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/services/telemetry_service.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory temp;
  late UiPrefs prefs;
  late FakeHttpClient http;
  File getLog() => File('${temp.path}/telemetry/events.log');

  setUp(() async {
    temp = Directory.systemTemp.createTempSync('fedi3-telemetry-test-');
    prefs = UiPrefs.defaults().copyWith(telemetryEnabled: true);
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
            const MethodChannel('plugins.flutter.io/path_provider'),
            (_) async => temp.path);
    http = FakeHttpClient();
    CoreHttp.useTestClient(http);
    await TelemetryService.init(() => prefs);
    await TelemetryService.clear();
  });
  tearDown(() async {
    await TelemetryService.clear();
    CoreHttp.reset();
    temp.deleteSync(recursive: true);
  });

  test(
      'telemetry stays local, handles concurrent writes and sends no HTTP requests',
      () async {
    await Future.wait(
        List.generate(20, (i) => TelemetryService.record('test', 'event $i')));
    expect(await TelemetryService.loadRecent(limit: 100), hasLength(20));
    expect(http.requests, isEmpty);
  });

  test('malformed and truncated JSON lines do not hide intact events',
      () async {
    await TelemetryService.record('test', 'intact event');
    await getLog()
        .writeAsString('not json\n{"truncated":', mode: FileMode.append);
    final events = await TelemetryService.loadRecent();
    expect(events, hasLength(1));
    expect(events.single.message, 'intact event');
    expect(await TelemetryService.loadRecent(limit: 0), isEmpty);
  });

  test('rotates oversized logs and clear removes both local copies', () async {
    await getLog().writeAsString('x' * (TelemetryService.maxLogBytes + 128));
    await TelemetryService.record('test', 'next file');
    expect(await getLog().length(), lessThan(TelemetryService.maxLogBytes));
    expect(await File('${getLog().path}.previous').length(),
        TelemetryService.maxLogBytes);
    final lines = await getLog().readAsLines();
    expect(jsonDecode(lines.single)['message'], 'next file');
    await TelemetryService.clear();
    expect(await getLog().exists(), isFalse);
    expect(await File('${getLog().path}.previous').exists(), isFalse);
  });

  test('disabled telemetry does not persist even forced diagnostic events',
      () async {
    prefs = UiPrefs.defaults();
    await TelemetryService.record('test', 'memory only', force: true);
    expect(await getLog().exists(), isFalse);
    expect(http.requests, isEmpty);
  });

  test('logging failures do not interrupt the caller', () async {
    final log = getLog();
    await Directory(log.path).create();
    await TelemetryService.record('test', 'must not throw');
    await Directory(log.path).delete();
  });
}
