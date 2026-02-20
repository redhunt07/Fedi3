/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/services.dart';
import 'package:media_kit/media_kit.dart';

class NotificationSoundService {
  NotificationSoundService._();

  static final AudioPlayer _generalPlayer = AudioPlayer(playerId: 'notify_general');
  static final AudioPlayer _chatPlayer = AudioPlayer(playerId: 'notify_chat');
  static Player? _winPlayer;
  static String? _winGeneralPath;
  static String? _winChatPath;
  static bool _ready = false;

  static Future<void> init() async {
    if (_ready) return;
    if (Platform.isWindows) {
      MediaKit.ensureInitialized();
      _winPlayer = Player();
      _ready = true;
      return;
    }
    _generalPlayer.setReleaseMode(ReleaseMode.stop);
    _chatPlayer.setReleaseMode(ReleaseMode.stop);
    _ready = true;
  }

  static Future<void> playGeneral() async {
    if (!_ready) await init();
    if (Platform.isWindows) {
      await _playWindows('assets/sounds/notify.mp3', isChat: false);
      return;
    }
    await _play(_generalPlayer, AssetSource('sounds/notify.mp3'));
  }

  static Future<void> playChat() async {
    if (!_ready) await init();
    if (Platform.isWindows) {
      await _playWindows('assets/sounds/chat.mp3', isChat: true);
      return;
    }
    await _play(_chatPlayer, AssetSource('sounds/chat.mp3'));
  }

  static Future<void> _play(AudioPlayer player, AssetSource source) async {
    try {
      await player.stop();
      await player.play(source, volume: 0.8);
    } catch (_) {
      // Ignore audio failures (e.g., missing backend).
    }
  }

  static Future<void> _playWindows(String assetPath, {required bool isChat}) async {
    final player = _winPlayer;
    if (player == null) return;
    try {
      final path = await _ensureWindowsAsset(assetPath, isChat: isChat);
      if (path == null) return;
      await player.stop();
      await player.open(Media(path), play: true);
      await player.setVolume(0.8);
    } catch (_) {
      // Ignore audio failures on Windows.
    }
  }

  static Future<String?> _ensureWindowsAsset(String assetPath, {required bool isChat}) async {
    final cached = isChat ? _winChatPath : _winGeneralPath;
    if (cached != null && File(cached).existsSync()) return cached;
    final bytes = await rootBundle.load(assetPath);
    final name = assetPath.split('/').last;
    final file = File('${Directory.systemTemp.path}/fedi3_$name');
    if (!file.existsSync()) {
      await file.writeAsBytes(bytes.buffer.asUint8List(), flush: true);
    }
    if (isChat) {
      _winChatPath = file.path;
    } else {
      _winGeneralPath = file.path;
    }
    return file.path;
  }
}
