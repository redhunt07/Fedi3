/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:flutter_local_notifications/flutter_local_notifications.dart';

class NotificationService {
  NotificationService._();

  static final FlutterLocalNotificationsPlugin _plugin =
      FlutterLocalNotificationsPlugin();
  static bool _ready = false;
  static final _taps = StreamController<String>.broadcast();
  static Stream<String> get taps => _taps.stream;
  static String? pendingThread;
  static void _onTap(NotificationResponse response) {
    final thread = response.payload;
    if (thread == null || thread.isEmpty || thread.length > 512) return;
    pendingThread = thread;
    _taps.add(thread);
  }

  static Future<void> init() async {
    if (_ready) return;
    const android = AndroidInitializationSettings('@mipmap/ic_launcher');
    const darwin = DarwinInitializationSettings();
    const linux = LinuxInitializationSettings(defaultActionName: 'Open');
    const settings = InitializationSettings(
      android: android,
      iOS: darwin,
      macOS: darwin,
      linux: linux,
    );
    try {
      await _plugin
          .initialize(settings, onDidReceiveNotificationResponse: _onTap)
          .timeout(const Duration(seconds: 3));
      _ready = true;
      final launch = await _plugin.getNotificationAppLaunchDetails();
      if (launch?.didNotificationLaunchApp ?? false) {
        final response = launch?.notificationResponse;
        if (response != null) _onTap(response);
      }
    } catch (_) {
      // Missing D-Bus/notification daemon must not prevent startup.
      _ready = false;
    }
  }

  static Future<void> showChatNotification(
      {required String title, required String body, String? threadId}) async {
    if (!_ready) return;
    const details = NotificationDetails(
      android: AndroidNotificationDetails(
        'fedi3_chat',
        'Chat',
        channelDescription: 'Chat messages',
        importance: Importance.high,
        priority: Priority.high,
      ),
      linux: LinuxNotificationDetails(),
      macOS: DarwinNotificationDetails(),
      iOS: DarwinNotificationDetails(),
    );
    try {
      if (Platform.isAndroid) {
        final allowed = await _plugin
            .resolvePlatformSpecificImplementation<
                AndroidFlutterLocalNotificationsPlugin>()
            ?.requestNotificationsPermission();
        if (allowed == false) return;
      }
      await _plugin.show(1001, title, body, details, payload: threadId);
    } catch (_) {/* Notification service may disappear after initialization. */}
  }
}
