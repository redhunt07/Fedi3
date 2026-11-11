/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

/// Process-wide marker that a call is active.
///
/// The UI isolate owns the call screen, while the push-only keep-alive service
/// runs in its own isolate and cannot read the UI's `coreHeld` flag. A small
/// file with a heartbeat timestamp bridges the two: the service keeps the core
/// alive while the marker is fresh, and stops it once the call ends or the
/// marker goes stale because the app was killed mid-call.
class CallActivityMarker {
  CallActivityMarker._();

  static const _fileName = 'call_active.json';
  static const _heartbeat = Duration(seconds: 20);
  static const _maxAge = Duration(seconds: 75);

  static Timer? _timer;
  static Directory? _directory;

  static Future<Directory> _dir() async =>
      _directory ??= await getApplicationSupportDirectory();

  static Future<File> _file() async =>
      File('${(await _dir()).path}${Platform.pathSeparator}$_fileName');

  /// Marks a call as active and keeps refreshing the heartbeat until [stop].
  static Future<void> start() async {
    await _write();
    _timer?.cancel();
    _timer = Timer.periodic(_heartbeat, (_) => unawaited(_write()));
  }

  /// Clears the marker; called when the call screen goes away.
  static Future<void> stop() async {
    _timer?.cancel();
    _timer = null;
    try {
      final file = await _file();
      if (await file.exists()) await file.delete();
    } catch (_) {}
  }

  /// True when a call is active, based on a heartbeat fresher than [maxAge].
  static Future<bool> isActive({Duration maxAge = _maxAge}) async {
    try {
      final file = await _file();
      if (!await file.exists()) return false;
      final raw = jsonDecode(await file.readAsString());
      if (raw is! Map) return false;
      final ts = raw['ts_ms'];
      if (ts is! num) return false;
      final age = DateTime.now().millisecondsSinceEpoch - ts.toInt();
      return age >= 0 && age <= maxAge.inMilliseconds;
    } catch (_) {
      return false;
    }
  }

  static Future<void> _write() async {
    try {
      final file = await _file();
      await file.writeAsString(
        jsonEncode({'ts_ms': DateTime.now().millisecondsSinceEpoch}),
      );
    } catch (e) {
      debugPrint('[call] marker write failed: $e');
    }
  }

  /// Overrides the directory in tests.
  @visibleForTesting
  static void useDirectory(Directory? directory) => _directory = directory;
}
