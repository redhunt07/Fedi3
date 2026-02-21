/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import '../core/core_api.dart';
import '../model/core_config.dart';
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
  static ValueGetter<CoreConfig?>? _configGetter;
  static String? _logPath;
  static final List<TelemetryEvent> _buffer = <TelemetryEvent>[];
  static DateTime? _lastRemoteSend;

  static Future<void> init(
    ValueGetter<UiPrefs> prefsGetter,
    ValueGetter<CoreConfig?> configGetter,
  ) async {
    _prefsGetter = prefsGetter;
    _configGetter = configGetter;
    _logPath = await _resolveLogPath();
    final msg = 'telemetry: logPath=${_logPath ?? "null"}';
    _safeDebug(msg);
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
    if (!force && enabled && !monitoringEnabled && !_isCriticalType(type)) {
      return;
    }
    // Add jitter to timestamp to prevent temporal correlation
    final jitteredTs = DateTime.now().toUtc().add(Duration(milliseconds: Random().nextInt(5000)));
    final event = TelemetryEvent(
      ts: jitteredTs,
      type: type,
      message: message.trim(),
      data: {
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
    if (enabled && _shouldSendRemote(event)) {
      unawaited(_sendRemote(event));
    }
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

  static bool _shouldSendRemote(TelemetryEvent event) {
    if (!_isCriticalType(event.type)) {
      return false;
    }
    final now = DateTime.now().toUtc();
    if (_lastRemoteSend != null && now.difference(_lastRemoteSend!).inSeconds < 10) {
      return false;
    }
    _lastRemoteSend = now;
    return true;
  }

  static bool _isCriticalType(String type) {
    final t = type.toLowerCase();
    return t.contains('error') ||
        t.contains('crash') ||
        t.contains('panic') ||
        t.contains('exception') ||
        t.contains('core_dead');
  }

  static String _sanitizeStack(String? stack) {
    if (stack == null || stack.trim().isEmpty) return '';
    final lines = <String>[];
    for (final raw in stack.split('\n')) {
      final line = raw.trim();
      if (line.isEmpty) continue;
      if (line.contains('package:') || line.contains('dart:')) {
        lines.add(line);
      } else if (line.contains('(')) {
        lines.add(line.split('(').first.trim());
      } else {
        lines.add(line);
      }
      if (lines.length >= 10) break;
    }
    return lines.join('\n');
  }

  static Future<void> _sendRemote(TelemetryEvent event) async {
    final cfg = _configGetter?.call();
    if (cfg == null) return;
    if (cfg.publicBaseUrl.trim().isEmpty || cfg.relayToken.trim().isEmpty) return;
    final host = Uri.tryParse(cfg.publicBaseUrl.trim())?.host.trim() ?? cfg.domain.trim();
    final handle = '@${cfg.username.trim()}@${host.isEmpty ? cfg.domain.trim() : host}';
    final payload = <String, dynamic>{
      'username': cfg.username.trim(),
      'type': event.type,
      'message': event.message,
      'stack': _sanitizeStack(event.data?['stack']?.toString()),
      'mode': event.data?['mode']?.toString() ?? (kReleaseMode ? 'release' : 'debug'),
      'ts': event.ts.toIso8601String(),
      'handle': handle,
    };
    try {
      final api = CoreApi(config: cfg);
      await api.sendClientTelemetry(payload);
    } catch (_) {
      // Best-effort: ignore failures.
    }
  }

  static Future<String?> _resolveLogPath() async {
    try {
      final dir = await getApplicationSupportDirectory();
      final msg = 'telemetry: supportDir=${dir.path}';
      _safeDebug(msg);
      final telemetryDir = Directory('${dir.path}${Platform.pathSeparator}telemetry');
      if (!await telemetryDir.exists()) {
        await telemetryDir.create(recursive: true);
      }
      return '${telemetryDir.path}${Platform.pathSeparator}events.log';
    } catch (e) {
      final msg = 'telemetry: resolveLogPath failed: $e';
      _safeDebug(msg);
      return null;
    }
  }

  static void _safeDebug(String msg) {
    try {
      if (kReleaseMode) return;
      debugPrint(msg);
    } catch (_) {
      // Ignore debug logging failures on GUI builds (e.g. Windows release).
    }
  }
}
