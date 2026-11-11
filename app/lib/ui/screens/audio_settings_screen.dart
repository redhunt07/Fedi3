/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:audioplayers/audioplayers.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:record/record.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/audio_devices.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';

/// Desktop audio routing for calls.
///
/// The same WebRTC audio device module used by [CallScreen] enumerates and
/// applies the devices here, so the microphone and speaker a user tests are the
/// ones a call opens. Linux playback uses `paplay --device` to reach the chosen
/// sink without changing the system-wide default.
class AudioSettingsScreen extends StatefulWidget {
  const AudioSettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<AudioSettingsScreen> createState() => _AudioSettingsScreenState();
}

class _AudioSettingsScreenState extends State<AudioSettingsScreen> {
  final AudioRecorder _recorder = AudioRecorder();
  // audioplayers_linux throws a C++ exception when GStreamer cannot create its
  // elements, which Dart cannot catch. Linux plays through paplay instead, the
  // same fallback SoundService uses.
  final AudioPlayer? _player = Platform.isLinux ? null : AudioPlayer();
  List<AudioDeviceOption> _inputs = const [];
  List<AudioDeviceOption> _outputs = const [];
  bool _loading = true;
  bool _testingMic = false;
  bool _playing = false;
  double _level = 0;
  String? _error;
  Timer? _micTimer;
  StreamSubscription<Amplitude>? _ampSub;
  String? _micPath;
  bool _usedDefaultMic = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  @override
  void dispose() {
    _micTimer?.cancel();
    _ampSub?.cancel();
    _recorder.dispose();
    _player?.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    await AudioDevices.ensureLabels();
    final inputs = await AudioDevices.inputs();
    final outputs = await AudioDevices.outputs();
    if (!mounted) return;
    setState(() {
      _inputs = inputs;
      _outputs = outputs;
      _loading = false;
    });
  }

  Future<void> _setInput(String id) async {
    await widget.appState.updatePrefs(
      (current) => current.copyWith(audioInputDeviceId: id),
    );
    try {
      // An empty selection means "system default": the native bridge keeps the
      // previously chosen device, so resolve and apply the current default.
      final effective = id.isEmpty ? await AudioDevices.defaultInputId() : id;
      if (effective != null) await AudioDevices.selectInput(effective);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
    if (mounted) setState(() {});
  }

  Future<void> _setOutput(String id) async {
    await widget.appState.updatePrefs(
      (current) => current.copyWith(audioOutputDeviceId: id),
    );
    try {
      final effective = id.isEmpty ? await AudioDevices.defaultOutputId() : id;
      if (effective != null) await AudioDevices.selectOutput(effective);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
    if (mounted) setState(() {});
  }

  Future<void> _startMicTest() async {
    if (_testingMic) {
      await _stopMicTest(playBack: true);
      return;
    }
    _micTimer?.cancel();
    final prefs = widget.appState.prefs;
    final selectedId = prefs.audioInputDeviceId;
    AudioDeviceOption? selected;
    for (final device in _inputs) {
      if (device.id == selectedId) {
        selected = device;
        break;
      }
    }
    try {
      final dir = await getTemporaryDirectory();
      _micPath =
          '${dir.path}/fedi3_mic_test_${DateTime.now().millisecondsSinceEpoch}.wav';
      // The WebRTC id is a friendly description on Linux; parecord needs the
      // technical source name, so resolve it before recording.
      final recordId = selectedId.isEmpty
          ? null
          : await AudioDevices.pulseDeviceName(selectedId, input: true);
      final config = RecordConfig(
        encoder: AudioEncoder.wav,
        sampleRate: 48000,
        numChannels: 1,
        device: recordId == null
            ? null
            : InputDevice(id: recordId, label: selected?.label ?? recordId),
      );
      _usedDefaultMic = false;
      try {
        await _recorder.start(config, path: _micPath!);
      } catch (_) {
        // The recorder may not expose the WebRTC id; the default device still
        // proves that audio capture works on this machine.
        _usedDefaultMic = true;
        await _recorder.start(
          const RecordConfig(
            encoder: AudioEncoder.wav,
            sampleRate: 48000,
            numChannels: 1,
          ),
          path: _micPath!,
        );
      }
      await _ampSub?.cancel();
      _ampSub = _recorder
          .onAmplitudeChanged(const Duration(milliseconds: 120))
          .listen(
        (amplitude) {
          if (!mounted) return;
          // dBFS: -60 is silence, 0 is full scale.
          final normalized = ((amplitude.current + 60) / 60).clamp(0.0, 1.0);
          setState(() => _level = normalized.toDouble());
        },
        onError: (_) {},
      );
      if (!mounted) return;
      setState(() {
        _testingMic = true;
        _error = null;
        _level = 0;
      });
      _micTimer = Timer(const Duration(seconds: 5), () {
        unawaited(_stopMicTest(playBack: true));
      });
    } catch (e) {
      if (mounted) {
        setState(() {
          _testingMic = false;
          _error = e.toString();
        });
      }
    }
  }

  Future<void> _stopMicTest({required bool playBack}) async {
    _micTimer?.cancel();
    _micTimer = null;
    await _ampSub?.cancel();
    _ampSub = null;
    String? path;
    try {
      path = await _recorder.stop();
    } catch (_) {}
    final result = path ?? _micPath;
    if (mounted) {
      setState(() {
        _testingMic = false;
        _level = 0;
      });
    }
    if (playBack && result != null) {
      await _playFile(result);
    }
  }

  Future<void> _testOutput() async {
    final prefs = widget.appState.prefs;
    if (prefs.audioOutputDeviceId.isNotEmpty) {
      try {
        await AudioDevices.selectOutput(prefs.audioOutputDeviceId);
      } catch (e) {
        if (mounted) setState(() => _error = e.toString());
      }
    }
    try {
      final data = await rootBundle.load('assets/sounds/call.wav');
      final dir = await getTemporaryDirectory();
      final path = '${dir.path}/fedi3_output_test.wav';
      await File(path).writeAsBytes(data.buffer.asUint8List(), flush: true);
      await _playFile(path);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  Future<void> _playFile(String path) async {
    final sink = widget.appState.prefs.audioOutputDeviceId;
    if (mounted) setState(() => _playing = true);
    try {
      if (Platform.isLinux) {
        // The stored id is the WebRTC friendly name; paplay needs the sink name.
        final sinkName =
            await AudioDevices.pulseDeviceName(sink, input: false) ?? sink;
        if (!await _playLinux(path, sinkName)) {
          throw 'paplay/pw-play could not play the test audio';
        }
      } else {
        final player = _player;
        if (player == null) throw 'audio playback is unavailable';
        await player.stop();
        await player.play(DeviceFileSource(path));
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _playing = false);
    }
  }

  /// Plays [path] on the selected sink without changing the system default.
  ///
  /// Returns false when neither PulseAudio nor PipeWire command is available.
  Future<bool> _playLinux(String path, String sink) async {
    final commands = <List<String>>[
      ['paplay', if (sink.isNotEmpty) '--device=$sink', path],
      ['pw-play', if (sink.isNotEmpty) '--target=$sink', path],
    ];
    for (final command in commands) {
      try {
        final result = await Process.run(command.first, command.sublist(1));
        if (result.exitCode == 0) return true;
      } on ProcessException {
        // The command is not installed; try the next player.
      }
    }
    return false;
  }

  String _selectedLabel(List<AudioDeviceOption> devices, String id) {
    if (id.isEmpty) return context.l10n.audioSystemDefault;
    for (final device in devices) {
      if (device.id == id) return device.label;
    }
    return context.l10n.audioDeviceMissing;
  }

  @override
  Widget build(BuildContext context) {
    final prefs = widget.appState.prefs;
    final scheme = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.audioSettingsTitle)),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
        children: [
          Text(
            context.l10n.audioSettingsHint,
            style: TextStyle(color: scheme.onSurfaceVariant, fontSize: 12),
          ),
          const SizedBox(height: 12),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 10),
              child: Text(_error!,
                  style: const TextStyle(color: Colors.redAccent)),
            ),
          if (_loading)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 24),
              child: Center(child: CircularProgressIndicator()),
            )
          else ...[
            _card(
              context,
              title: context.l10n.audioInputTitle,
              subtitle: context.l10n.audioInputHint,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  _deviceDropdown(
                    context,
                    value: prefs.audioInputDeviceId,
                    devices: _inputs,
                    onChanged: _setInput,
                  ),
                  if (_inputs.isEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 6),
                      child: Text(context.l10n.audioNoDevices,
                          style: TextStyle(
                              color: scheme.onSurfaceVariant, fontSize: 12)),
                    ),
                  const SizedBox(height: 10),
                  Row(
                    children: [
                      FilledButton.icon(
                        onPressed: _startMicTest,
                        icon: Icon(_testingMic
                            ? Icons.stop_rounded
                            : Icons.mic_rounded),
                        label: Text(_testingMic
                            ? context.l10n.audioStopTest
                            : context.l10n.audioTestMic),
                      ),
                      const SizedBox(width: 12),
                      if (_testingMic)
                        Expanded(
                          child: ClipRRect(
                            borderRadius: BorderRadius.circular(6),
                            child: LinearProgressIndicator(
                              value: _level,
                              minHeight: 8,
                              backgroundColor: scheme.primary.withAlpha(30),
                            ),
                          ),
                        ),
                    ],
                  ),
                  if (_usedDefaultMic && _testingMic)
                    Padding(
                      padding: const EdgeInsets.only(top: 6),
                      child: Text(context.l10n.audioDefaultMicHint,
                          style: TextStyle(
                              color: scheme.onSurfaceVariant, fontSize: 12)),
                    ),
                  const SizedBox(height: 6),
                  Text(context.l10n.audioTestMicHint,
                      style: TextStyle(
                          color: scheme.onSurfaceVariant, fontSize: 12)),
                ],
              ),
            ),
            const SizedBox(height: 12),
            _card(
              context,
              title: context.l10n.audioOutputTitle,
              subtitle: context.l10n.audioOutputHint,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  _deviceDropdown(
                    context,
                    value: prefs.audioOutputDeviceId,
                    devices: _outputs,
                    onChanged: _setOutput,
                  ),
                  if (_outputs.isEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 6),
                      child: Text(context.l10n.audioNoDevices,
                          style: TextStyle(
                              color: scheme.onSurfaceVariant, fontSize: 12)),
                    ),
                  const SizedBox(height: 10),
                  FilledButton.icon(
                    onPressed: _playing ? null : _testOutput,
                    icon: Icon(_playing
                        ? Icons.graphic_eq_rounded
                        : Icons.volume_up_rounded),
                    label: Text(context.l10n.audioTestOutput),
                  ),
                  const SizedBox(height: 6),
                  Text(context.l10n.audioTestOutputHint,
                      style: TextStyle(
                          color: scheme.onSurfaceVariant, fontSize: 12)),
                  const SizedBox(height: 8),
                  Text(context.l10n.audioScreenShareHint,
                      style: TextStyle(
                          color: scheme.onSurfaceVariant, fontSize: 11)),
                ],
              ),
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                TextButton.icon(
                  onPressed: _loading ? null : _load,
                  icon: const Icon(Icons.refresh_rounded),
                  label: Text(context.l10n.audioRefresh),
                ),
              ],
            ),
            Text(
              context.l10n.audioAppliedHint,
              style: TextStyle(color: scheme.onSurfaceVariant, fontSize: 12),
            ),
          ],
        ],
      ),
    );
  }

  Widget _card(
    BuildContext context, {
    required String title,
    required String subtitle,
    required Widget child,
  }) {
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: const Color(UiTokens.darkSurface),
        borderRadius: BorderRadius.circular(UiTokens.radiusCard),
        border: Border.all(color: const Color(UiTokens.darkBorder)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title,
              style:
                  const TextStyle(fontWeight: FontWeight.w800, fontSize: 14)),
          const SizedBox(height: 2),
          Text(subtitle,
              style: TextStyle(
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                  fontSize: 12)),
          const SizedBox(height: 10),
          child,
        ],
      ),
    );
  }

  Widget _deviceDropdown(
    BuildContext context, {
    required String value,
    required List<AudioDeviceOption> devices,
    required ValueChanged<String> onChanged,
  }) {
    // A saved device can disappear (unplugged headset). Keep it visible so the
    // user understands why the default is used instead of silently resetting.
    final ids = <String>{''};
    for (final device in devices) {
      ids.add(device.id);
    }
    final stale = value.isNotEmpty && !ids.contains(value);
    final entries = <DropdownMenuItem<String>>[
      DropdownMenuItem(
        value: '',
        child: Text(context.l10n.audioSystemDefault),
      ),
      ...devices.map((device) => DropdownMenuItem(
            value: device.id,
            child: Text(device.label, overflow: TextOverflow.ellipsis),
          )),
      if (stale)
        DropdownMenuItem(
          value: value,
          child: Text(context.l10n.audioDeviceMissing,
              overflow: TextOverflow.ellipsis),
        ),
    ];
    return InputDecorator(
      decoration: InputDecoration(
        border: const OutlineInputBorder(),
        isDense: true,
        helperText: _selectedLabel(devices, value),
      ),
      child: DropdownButtonHideUnderline(
        child: DropdownButton<String>(
          value: value,
          isExpanded: true,
          isDense: true,
          items: entries,
          onChanged: (next) {
            if (next != null) onChanged(next);
          },
        ),
      ),
    );
  }
}
