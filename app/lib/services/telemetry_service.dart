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
    if (tsRaw == null || type == null || type.isEmpty || message == null || message.isEmpty) {
      return null;
    }
    final ts = DateTime.tryParse(tsRaw) ?? DateTime.now();
    final data = raw['data'] is Map ? Map<String, dynamic>.from(raw['data'] as Map) : null;
    return TelemetryEvent(ts: ts, type: type, message: message, data: data);
  }
}

class TelemetryService {
  static ValueGetter<UiPrefs>? _prefsGetter;
  static String? _logPath;
  static final List<TelemetryEvent> _buffer = <TelemetryEvent>[];

  static Future<void> init(ValueGetter<UiPrefs> prefsGetter) async {
    _prefsGetter = prefsGetter;
    _logPath = await _resolveLogPath();
  }

  static bool get enabled => _prefsGetter?.call().telemetryEnabled ?? false;
  static bool get monitoringEnabled => _prefsGetter?.call().clientMonitoringEnabled ?? false;

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
    final path = _logPath ?? await _resolveLogPath();
    if (path == null) return;
    final file = File(path);
    final line = jsonEncode(event.toJson());
    await file.writeAsString('$line\n', mode: FileMode.append, flush: false);
  }

  static Future<List<TelemetryEvent>> loadRecent({int limit = 100}) async {
    final path = _logPath ?? await _resolveLogPath();
    if (path == null) return List.unmodifiable(_buffer.reversed);
    final file = File(path);
    if (!await file.exists()) {
      return List.unmodifiable(_buffer.reversed);
    }
    final lines = await file.readAsLines();
    final start = lines.length > limit ? lines.length - limit : 0;
    final events = <TelemetryEvent>[];
    for (final line in lines.sublist(start)) {
      final raw = jsonDecode(line);
      if (raw is Map<String, dynamic>) {
        final ev = TelemetryEvent.fromJson(raw);
        if (ev != null) {
          events.add(ev);
        }
      }
    }
    return events.reversed.toList(growable: false);
  }

  static Future<void> clear() async {
    _buffer.clear();
    final path = _logPath ?? await _resolveLogPath();
    if (path == null) return;
    final file = File(path);
    if (await file.exists()) {
      await file.delete();
    }
  }

  static Future<File?> exportLog() async {
    final path = _logPath ?? await _resolveLogPath();
    if (path == null) return null;
    final file = File(path);
    if (!await file.exists()) return null;
    final dir = await getTemporaryDirectory();
    final ts = DateTime.now().toUtc().toIso8601String().replaceAll(':', '-');
    final out = File('${dir.path}${Platform.pathSeparator}fedi3-telemetry-$ts.log');
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
      final telemetryDir = Directory('${dir.path}${Platform.pathSeparator}telemetry');
      if (!await telemetryDir.exists()) {
        await telemetryDir.create(recursive: true);
      }
      return '${telemetryDir.path}${Platform.pathSeparator}events.log';
    } catch (_) {
      return null;
    }
  }
}
