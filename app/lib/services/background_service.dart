/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:flutter/services.dart';

/// Android foreground service that keeps the app process alive while the UI is
/// in the background. It polls the relay spool for new activity and starts the
/// local core only when there is something to deliver, so notifications keep
/// arriving without a push provider and without an idle core or P2P swarm.
class BackgroundService {
  BackgroundService._();

  static const MethodChannel _channel =
      MethodChannel('org.fedi3.client/keepalive');

  static bool get supported => Platform.isAndroid;

  static Future<bool> isEnabled() async {
    if (!supported) return false;
    try {
      return await _channel.invokeMethod<bool>('isEnabled') ?? false;
    } catch (_) {
      return false;
    }
  }

  static Future<bool> setEnabled(bool enabled, {String? statusText}) async {
    if (!supported) return false;
    try {
      return await _channel.invokeMethod<bool>('setEnabled', {
            'enabled': enabled,
            'text': statusText,
          }) ??
          enabled;
    } catch (_) {
      return enabled;
    }
  }

  static Future<bool> isRunning() async {
    if (!supported) return false;
    try {
      return await _channel.invokeMethod<bool>('isRunning') ?? false;
    } catch (_) {
      return false;
    }
  }
}
