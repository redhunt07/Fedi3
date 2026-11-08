/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';

import 'package:audioplayers/audioplayers.dart';

/// Short notification sounds bundled with the app.
///
/// Playback is best-effort: missing audio devices or a missing plugin in
/// widget tests must never break the UI.
class SoundService {
  SoundService._();

  static final AudioPlayer? _activityPlayer =
      Platform.isLinux ? null : AudioPlayer();
  static final AudioPlayer? _chatPlayer =
      Platform.isLinux ? null : AudioPlayer();
  static final AudioPlayer? _callPlayer =
      Platform.isLinux ? null : AudioPlayer();
  static int _callGeneration = 0;

  // audioplayers_linux throws C++ exceptions when GStreamer elements are
  // unavailable. Dart cannot catch these: keep Linux playback in a subprocess.
  static final Map<String, Future<File>> _linuxAssets = {};
  static final Map<String, Process> _linuxProcesses = {};

  static Future<File> _linuxAsset(String asset) =>
      _linuxAssets.putIfAbsent(asset, () async {
        final data = await rootBundle.load('assets/$asset');
        final dir = await Directory.systemTemp.createTemp('fedi3_sound_');
        return File('${dir.path}/sound.wav').writeAsBytes(
          data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
          flush: true,
        );
      });

  static Future<void> _playLinux(String asset, {int? generation}) async {
    try {
      final file = await _linuxAsset(asset);
      if (generation != null && generation != _callGeneration) return;
      _linuxProcesses.remove(asset)?.kill();
      Process? process;
      for (final command in ['paplay', 'pw-play', 'aplay']) {
        try {
          process = await Process.start(command, [file.path]);
          break;
        } on ProcessException {
          /* Try the next installed player. */
        }
      }
      if (process == null) return;
      if (generation != null && generation != _callGeneration) {
        process.kill();
        return;
      }
      _linuxProcesses[asset] = process;
      unawaited(process.stdout.drain<void>());
      unawaited(process.stderr.drain<void>());
      final code = await process.exitCode;
      if (identical(_linuxProcesses[asset], process)) {
        _linuxProcesses.remove(asset);
      }
      if (code == 0 && generation != null && generation == _callGeneration) {
        unawaited(_playLinux(asset, generation: generation));
      }
    } catch (_) {
      /* Audio is optional. */
    }
  }

  static Future<void> _playOnce(AudioPlayer? player, String asset) async {
    if (Platform.isLinux) return _playLinux(asset);
    if (player == null) return;
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
    if (Platform.isLinux) {
      return _playLinux('sounds/call.wav', generation: generation);
    }
    final player = _callPlayer;
    if (player == null) return;
    try {
      await player.stop();
      if (generation != _callGeneration) return;
      await player.setReleaseMode(ReleaseMode.loop);
      if (generation != _callGeneration) return;
      await player.play(AssetSource('sounds/call.wav'), volume: 0.8);
    } catch (_) {
      // Audio is optional.
    }
  }

  static Future<void> stopCall() async {
    _callGeneration++;
    if (Platform.isLinux) {
      _linuxProcesses.remove('sounds/call.wav')?.kill();
      return;
    }
    final player = _callPlayer;
    if (player == null) return;
    try {
      await player.stop();
    } catch (_) {
      // Audio is optional.
    }
  }
}
