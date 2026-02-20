/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:local_notifier/local_notifier.dart';

import 'notification_sound_service.dart';

class NotificationService {
  NotificationService._();

  static final FlutterLocalNotificationsPlugin _plugin = FlutterLocalNotificationsPlugin();
  static bool _ready = false;
  static bool _localReady = false;

  static Future<void> init() async {
    if (_ready) return;
    await NotificationSoundService.init();
    if (Platform.isWindows || Platform.isLinux) {
      try {
        await localNotifier.setup(appName: 'Fedi3');
        _localReady = true;
      } catch (_) {
        _localReady = false;
      }
      _ready = false;
      return;
    }
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
      final ok = await _plugin.initialize(settings);
      _ready = ok ?? true;
    } catch (_) {
      _ready = false;
    }
  }

  static Future<void> showChatNotification({required String title, required String body}) async {
    if (Platform.isWindows || Platform.isLinux) {
      await _showLocal(title: title, body: body, chat: true);
      return;
    }
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
    await _plugin.show(1001, title, body, details);
  }

  static Future<void> showGeneralNotification({required String title, required String body}) async {
    if (Platform.isWindows || Platform.isLinux) {
      await _showLocal(title: title, body: body, chat: false);
      return;
    }
    if (!_ready) return;
    const details = NotificationDetails(
      android: AndroidNotificationDetails(
        'fedi3_general',
        'General',
        channelDescription: 'General notifications',
        importance: Importance.defaultImportance,
        priority: Priority.defaultPriority,
      ),
      linux: LinuxNotificationDetails(),
      macOS: DarwinNotificationDetails(),
      iOS: DarwinNotificationDetails(),
    );
    await _plugin.show(1002, title, body, details);
  }

  static Future<void> _showLocal({required String title, required String body, required bool chat}) async {
    if (!_localReady) return;
    try {
      final notification = LocalNotification(
        title: title,
        body: body,
      );
      await notification.show();
      if (chat) {
        await NotificationSoundService.playChat();
      } else {
        await NotificationSoundService.playGeneral();
      }
    } catch (_) {
      // Ignore notification failures.
    }
  }
}
