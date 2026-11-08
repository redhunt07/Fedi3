/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:flutter_webrtc/flutter_webrtc.dart';

/// One desktop audio endpoint exposed by the WebRTC audio device module.
///
/// The id is the value the call pipeline accepts, so the device a user tests
/// here is exactly the device a call will use.
class AudioDeviceOption {
  const AudioDeviceOption({required this.id, required this.label});

  final String id;
  final String label;
}

/// Desktop audio device discovery and selection.
///
/// Enumeration goes through the same WebRTC audio device module used by calls;
/// a separate OS list would let users pick a device the call never opens.
class AudioDevices {
  AudioDevices._();

  /// Enumerates microphones. Call [ensureLabels] first on platforms that hide
  /// labels until microphone permission was granted once.
  static Future<List<AudioDeviceOption>> inputs() => _list('audioinput');

  /// Enumerates speakers and headphones.
  static Future<List<AudioDeviceOption>> outputs() => _list('audiooutput');

  /// Opens and immediately closes the microphone so labels become available.
  static Future<void> ensureLabels() async {
    try {
      final stream = await navigator.mediaDevices.getUserMedia({'audio': true});
      for (final track in stream.getTracks()) {
        await track.stop();
      }
      await stream.dispose();
    } catch (_) {
      // Enumeration still works without labels.
    }
  }

  /// Applies the microphone used for calls. An empty id is a no-op because the
  /// native bridge rejects unknown device ids.
  static Future<void> selectInput(String id) async {
    final value = id.trim();
    if (value.isEmpty) return;
    await Helper.selectAudioInput(value);
  }

  /// Applies the speaker or headphone output used for calls.
  static Future<void> selectOutput(String id) async {
    final value = id.trim();
    if (value.isEmpty) return;
    await Helper.selectAudioOutput(value);
  }

  /// Resolves the operating system's default microphone.
  ///
  /// The native bridge keeps the last selected device, so switching back to
  /// "system default" must select the current default explicitly. Only Linux
  /// exposes it through a stable command (`pactl`); elsewhere the ADM already
  /// follows the OS default.
  static Future<String?> defaultInputId() =>
      _defaultId(['get-default-source'], input: true);

  /// Resolves the operating system's default output.
  static Future<String?> defaultOutputId() =>
      _defaultId(['get-default-sink'], input: false);

  /// Maps a WebRTC device id to the PulseAudio/PipeWire name used by `paplay`
  /// and `parecord`.
  ///
  /// On Linux the WebRTC module reports friendly descriptions as ids, while the
  /// command line players need the technical sink/source name.
  static Future<String?> pulseDeviceName(String id,
      {required bool input}) async {
    final value = id.trim();
    if (!Platform.isLinux || value.isEmpty) return value;
    final entries = await _pulseEntries(input: input);
    for (final entry in entries) {
      if (entry.description == value || entry.name == value) return entry.name;
    }
    return value;
  }

  static Future<String?> _defaultId(
    List<String> args, {
    required bool input,
  }) async {
    if (!Platform.isLinux) return null;
    final name = await _pactl(args);
    if (name == null) return null;
    final entries = await _pulseEntries(input: input);
    String? description;
    for (final entry in entries) {
      if (entry.name == name) {
        description = entry.description;
        break;
      }
    }
    final devices = input ? await inputs() : await outputs();
    for (final device in devices) {
      if (device.id == name ||
          device.label == name ||
          (description != null && device.label == description)) {
        return device.id;
      }
    }
    return null;
  }

  static Future<String?> _pactl(List<String> args) async {
    try {
      final result = await Process.run('pactl', args);
      final value = (result.stdout as String).trim();
      if (result.exitCode != 0 || value.isEmpty) return null;
      return value;
    } catch (_) {
      // pactl is missing (e.g. a bare ALSA setup); keep the ADM selection.
      return null;
    }
  }

  static Future<List<_PulseEntry>> _pulseEntries({required bool input}) async {
    try {
      final result = await Process.run(
        'pactl',
        ['list', input ? 'sources' : 'sinks'],
        environment: const {'LC_ALL': 'C'},
      );
      if (result.exitCode != 0) return const [];
      final entries = <_PulseEntry>[];
      String? name;
      String? description;
      for (final raw in (result.stdout as String).split('\n')) {
        final line = raw.trim();
        if (line.startsWith('Source #') || line.startsWith('Sink #')) {
          if (name != null) {
            entries.add(_PulseEntry(name, description ?? name));
          }
          name = null;
          description = null;
        } else if (line.startsWith('Name: ')) {
          name = line.substring(6).trim();
        } else if (line.startsWith('Description: ')) {
          description = line.substring(13).trim();
        }
      }
      if (name != null) {
        entries.add(_PulseEntry(name, description ?? name));
      }
      return entries;
    } catch (_) {
      return const [];
    }
  }

  static Future<List<AudioDeviceOption>> _list(String kind) async {
    final seen = <String>{};
    final result = <AudioDeviceOption>[];
    try {
      final devices = await navigator.mediaDevices.enumerateDevices();
      for (final device in devices) {
        if (device.kind != kind) continue;
        final id = device.deviceId.trim();
        if (id.isEmpty || !seen.add(id)) continue;
        final label = device.label.trim();
        result.add(AudioDeviceOption(
          id: id,
          label: label.isEmpty
              ? '${kind == 'audioinput' ? 'Microphone' : 'Output'} ${result.length + 1}'
              : label,
        ));
      }
    } catch (_) {
      // A missing audio backend leaves the list empty; callers show a hint.
    }
    return result;
  }
}

class _PulseEntry {
  const _PulseEntry(this.name, this.description);

  final String name;
  final String description;
}
