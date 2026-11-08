/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:flutter/services.dart';

/// Starts and stops the Android foreground service required before a
/// MediaProjection (screen sharing) can be created. Android throws a
/// SecurityException and kills the app when the projection starts without a
/// running `mediaProjection` foreground service.
class ScreenShareService {
  ScreenShareService._();

  static const MethodChannel _channel =
      MethodChannel('org.fedi3.client/keepalive');

  static bool get supported => Platform.isAndroid;

  static Future<void> start() async {
    if (!supported) return;
    try {
      await _channel.invokeMethod<bool>('startScreenShare');
    } catch (_) {
      // Best effort: on older devices the service is not required.
    }
  }

  static Future<void> stop() async {
    if (!supported) return;
    try {
      await _channel.invokeMethod<bool>('stopScreenShare');
    } catch (_) {
      // Ignore: stopping an already stopped service is harmless.
    }
  }
}
