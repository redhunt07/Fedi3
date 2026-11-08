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

  static Future<void> requestPermission() async {
    if (Platform.isAndroid) {
      await _plugin
          .resolvePlatformSpecificImplementation<
              AndroidFlutterLocalNotificationsPlugin>()
          ?.requestNotificationsPermission();
    }
  }

  static Future<void> showActivityNotification(
      {required String title, required String body}) async {
    if (!_ready) return;
    const details = NotificationDetails(
      android: AndroidNotificationDetails(
        'fedi3_activity_v2',
        'Activity',
        channelDescription: 'Timeline activity',
        importance: Importance.defaultImportance,
        priority: Priority.defaultPriority,
        sound: RawResourceAndroidNotificationSound('fedi3_activity'),
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
            ?.areNotificationsEnabled();
        if (allowed == false) return;
      }
      await _plugin.show(1002, title, body, details);
    } catch (_) {/* Notification service may disappear after initialization. */}
  }

  static Future<void> showChatNotification(
      {required String title, required String body, String? threadId}) async {
    if (!_ready) return;
    const details = NotificationDetails(
      android: AndroidNotificationDetails(
        'fedi3_chat_v2',
        'Chat',
        channelDescription: 'Chat messages',
        importance: Importance.high,
        priority: Priority.high,
        sound: RawResourceAndroidNotificationSound('fedi3_chat'),
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
            ?.areNotificationsEnabled();
        if (allowed == false) return;
      }
      await _plugin.show(1001, title, body, details, payload: threadId);
    } catch (_) {/* Notification service may disappear after initialization. */}
  }

  /// Rings for an incoming call while the app is in the background. The tap
  /// brings the app forward and the shell then shows its call prompt.
  static Future<void> showCallNotification(
      {required String title, required String body, String? threadId}) async {
    if (!_ready) return;
    const details = NotificationDetails(
      android: AndroidNotificationDetails(
        'fedi3_call_v2',
        'Calls',
        channelDescription: 'Incoming calls',
        importance: Importance.max,
        priority: Priority.max,
        category: AndroidNotificationCategory.call,
        fullScreenIntent: true,
        sound: RawResourceAndroidNotificationSound('fedi3_call'),
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
            ?.areNotificationsEnabled();
        if (allowed == false) return;
      }
      await _plugin.show(1002, title, body, details, payload: threadId);
    } catch (_) {/* Notification service may disappear after initialization. */}
  }

  static Future<void> cancelCallNotification() async {
    if (!_ready) return;
    try {
      await _plugin.cancel(1002);
    } catch (_) {}
  }
}
