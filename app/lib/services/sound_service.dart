/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:audioplayers/audioplayers.dart';

/// Short notification sounds bundled with the app.
///
/// Playback is best-effort: missing audio devices or a missing plugin in
/// widget tests must never break the UI.
class SoundService {
  SoundService._();

  static final AudioPlayer _activityPlayer = AudioPlayer();
  static final AudioPlayer _chatPlayer = AudioPlayer();
  static final AudioPlayer _callPlayer = AudioPlayer();
  static int _callGeneration = 0;

  static Future<void> _playOnce(AudioPlayer player, String asset) async {
    try {
      await player.stop();
      await player.setReleaseMode(ReleaseMode.stop);
      await player.play(AssetSource(asset), volume: 0.8);
    } catch (_) {
      // Audio is optional; never let a playback failure surface in the UI.
    }
  }

  static Future<void> playActivity() =>
      _playOnce(_activityPlayer, 'sounds/activity.wav');

  static Future<void> playChat() => _playOnce(_chatPlayer, 'sounds/chat.wav');

  static Future<void> playCallLoop() async {
    final generation = ++_callGeneration;
    try {
      await _callPlayer.stop();
      if (generation != _callGeneration) return;
      await _callPlayer.setReleaseMode(ReleaseMode.loop);
      if (generation != _callGeneration) return;
      await _callPlayer.play(AssetSource('sounds/call.wav'), volume: 0.8);
    } catch (_) {
      // Audio is optional.
    }
  }

  static Future<void> stopCall() async {
    _callGeneration++;
    try {
      await _callPlayer.stop();
    } catch (_) {
      // Audio is optional.
    }
  }
}
