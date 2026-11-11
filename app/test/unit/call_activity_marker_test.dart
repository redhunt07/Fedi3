/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:fedi3/services/call_activity_marker.dart';

void main() {
  late Directory dir;

  setUp(() async {
    dir = await Directory.systemTemp.createTemp('fedi3_call_marker_');
    CallActivityMarker.useDirectory(dir);
  });

  tearDown(() async {
    CallActivityMarker.useDirectory(null);
    await CallActivityMarker.stop();
    await dir.delete(recursive: true);
  });

  test('inactive without a marker file', () async {
    expect(await CallActivityMarker.isActive(), isFalse);
  });

  test('active right after start and inactive after stop', () async {
    await CallActivityMarker.start();
    expect(await CallActivityMarker.isActive(), isTrue);
    await CallActivityMarker.stop();
    expect(await CallActivityMarker.isActive(), isFalse);
  });

  test('stale heartbeat is not considered active', () async {
    final file = File('${dir.path}/call_active.json');
    await file.writeAsString(
      jsonEncode({
        'ts_ms': DateTime.now()
            .subtract(const Duration(minutes: 5))
            .millisecondsSinceEpoch,
      }),
    );
    expect(await CallActivityMarker.isActive(), isFalse);
  });

  test('a fresh heartbeat from another isolate counts as active', () async {
    final file = File('${dir.path}/call_active.json');
    await file.writeAsString(
      jsonEncode({'ts_ms': DateTime.now().millisecondsSinceEpoch}),
    );
    expect(await CallActivityMarker.isActive(), isTrue);
  });
}
