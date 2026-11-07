/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import '../model/ui_prefs.dart';

class TelemetryEvent {
  const TelemetryEvent({
    required this.ts,
    required this.type,
    required this.message,
    this.data,
  });

  final DateTime ts;
  final String type;
  final String message;
  final Map<String, dynamic>? data;

  Map<String, dynamic> toJson() => {
        'ts': ts.toIso8601String(),
        'type': type,
        'message': message,
        if (data != null) 'data': data,
      };

  static TelemetryEvent? fromJson(Map<String, dynamic> raw) {
    final tsRaw = raw['ts']?.toString();
    final type = raw['type']?.toString().trim();
    final message = raw['message']?.toString().trim();
    if (tsRaw == null ||
        type == null ||
        type.isEmpty ||
        message == null ||
        message.isEmpty) {
      return null;
    }
    final ts = DateTime.tryParse(tsRaw) ?? DateTime.now();
    final data = raw['data'] is Map
        ? Map<String, dynamic>.from(raw['data'] as Map)
        : null;
    return TelemetryEvent(ts: ts, type: type, message: message, data: data);
  }
}

/// Local diagnostics only. There is no network transport or GitHub upload.
class TelemetryService {
  static const maxLogBytes = 2 * 1024 * 1024;
  static Future<void> _writes = Future<void>.value();

  static Future<void> _enqueue(Future<void> Function() action) {
    final operation = _writes.then((_) => action()).catchError((Object _) {
      // Diagnostic I/O must never break the operation being diagnosed.
    });
    _writes = operation;
    return operation;
  }

  static ValueGetter<UiPrefs>? _prefsGetter;
  static String? _logPath;
  static final List<TelemetryEvent> _buffer = <TelemetryEvent>[];

  static Future<void> init(ValueGetter<UiPrefs> prefsGetter) async {
    _prefsGetter = prefsGetter;
    _logPath = await _resolveLogPath();
  }

  static bool get enabled => _prefsGetter?.call().telemetryEnabled ?? false;
  static bool get monitoringEnabled =>
      _prefsGetter?.call().clientMonitoringEnabled ?? false;

  static Future<void> record(
    String type,
    String message, {
    Map<String, dynamic>? data,
    bool force = false,
  }) async {
    if (!force && !(enabled || monitoringEnabled || kDebugMode)) {
      return;
    }
    final event = TelemetryEvent(
      ts: DateTime.now().toUtc(),
      type: type,
      message: message.trim(),
      data: {
        'platform': Platform.operatingSystem,
        'mode': kReleaseMode ? 'release' : 'debug',
        if (data != null) ...data,
      },
    );
    _pushBuffer(event);
    if (!(enabled || monitoringEnabled)) {
      return;
    }
    await _enqueue(() async {
      if (!(enabled || monitoringEnabled)) return;
      final path = _logPath ?? await _resolveLogPath();
      if (path == null) return;
      final file = File(path);
      final line = '${jsonEncode(event.toJson())}\n';
      // Skip pathological events rather than let one entry exceed the log cap.
      final bytes = utf8.encode(line);
      if (bytes.length > maxLogBytes) return;
      if (await file.exists() &&
          await file.length() + bytes.length > maxLogBytes) {
        final previous = File('$path.previous');
        if (await previous.exists()) await previous.delete();
        final length = await file.length();
        if (length > maxLogBytes) {
          // Bound legacy logs too: old clients did not rotate at all.
          final input = await file.open();
          try {
            await input.setPosition(length - maxLogBytes);
            await previous.writeAsBytes(await input.read(maxLogBytes));
          } finally {
            await input.close();
          }
          await file.delete();
        } else {
          await file.rename(previous.path);
        }
      }
      await file.writeAsBytes(bytes, mode: FileMode.append, flush: false);
    });
  }

  static Future<List<TelemetryEvent>> loadRecent({int limit = 100}) async {
    if (limit <= 0) return const [];
    await _writes;
    final fallback = _buffer.reversed.take(limit).toList(growable: false);
    try {
      final path = _logPath ?? await _resolveLogPath();
      if (path == null) return fallback;
      final file = File(path);
      if (!await file.exists()) return fallback;
      final length = await file.length();
      final start = length > maxLogBytes ? length - maxLogBytes : 0;
      final input = await file.open();
      late final List<int> bytes;
      try {
        await input.setPosition(start);
        bytes = await input.read(maxLogBytes);
      } finally {
        await input.close();
      }
      final lines = const LineSplitter()
          .convert(utf8.decode(bytes, allowMalformed: true));
      final events = <TelemetryEvent>[];
      // Skip an incomplete first line when reading the tail of a legacy log.
      for (final line in lines.skip(start > 0 ? 1 : 0).toList().reversed) {
        try {
          final raw = jsonDecode(line);
          if (raw is Map<String, dynamic>) {
            final event = TelemetryEvent.fromJson(raw);
            if (event != null) events.add(event);
          }
        } on FormatException {
          // A crash can truncate the last line; keep the remaining diagnostics.
        }
        if (events.length >= limit) break;
      }
      return events;
    } catch (_) {
      return fallback;
    }
  }

  static Future<void> clear() async {
    _buffer.clear();
    await _enqueue(() async {
      final path = _logPath ?? await _resolveLogPath();
      if (path == null) return;
      for (final name in [path, '$path.previous']) {
        final file = File(name);
        if (await file.exists()) await file.delete();
      }
    });
  }

  static Future<File?> exportLog() async {
    await _writes;
    final path = _logPath ?? await _resolveLogPath();
    if (path == null) return null;
    final file = File(path);
    if (!await file.exists()) return null;
    final dir = await getTemporaryDirectory();
    final ts = DateTime.now().toUtc().toIso8601String().replaceAll(':', '-');
    final out =
        File('${dir.path}${Platform.pathSeparator}fedi3-telemetry-$ts.log');
    await file.copy(out.path);
    return out;
  }

  static void _pushBuffer(TelemetryEvent event) {
    _buffer.add(event);
    if (_buffer.length > 200) {
      _buffer.removeAt(0);
    }
  }

  static Future<String?> _resolveLogPath() async {
    try {
      final dir = await getApplicationSupportDirectory();
      final telemetryDir =
          Directory('${dir.path}${Platform.pathSeparator}telemetry');
      if (!await telemetryDir.exists()) {
        await telemetryDir.create(recursive: true);
      }
      return '${telemetryDir.path}${Platform.pathSeparator}events.log';
    } catch (_) {
      return null;
    }
  }
}
