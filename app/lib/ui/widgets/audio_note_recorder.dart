/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:record/record.dart';

import '../../l10n/l10n_ext.dart';
import '../theme/ui_tokens.dart';

/// Records an audio note with the platform recorder (PulseAudio on Linux).
class AudioNoteRecorder extends StatefulWidget {
  const AudioNoteRecorder({super.key});

  @override
  State<AudioNoteRecorder> createState() => _AudioNoteRecorderState();
}

class _AudioNoteRecorderState extends State<AudioNoteRecorder> {
  final AudioRecorder _recorder = AudioRecorder();
  String? _path;
  String? _error;
  bool _starting = true;
  bool _stopping = false;
  Timer? _ticker;
  int _seconds = 0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _start());
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _recorder.dispose();
    super.dispose();
  }

  Future<void> _start() async {
    try {
      final permissionMessage = context.l10n.callNoPermission;
      if (!await _recorder.hasPermission()) {
        throw Exception(permissionMessage);
      }
      final dir = await getTemporaryDirectory();
      _path =
          '${dir.path}/fedi3_note_${DateTime.now().microsecondsSinceEpoch}.m4a';
      await _recorder.start(
        const RecordConfig(encoder: AudioEncoder.aacLc),
        path: _path!,
      );
      _ticker = Timer.periodic(const Duration(seconds: 1), (_) {
        if (mounted) setState(() => _seconds++);
      });
      if (mounted) setState(() => _starting = false);
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = e.toString();
          _starting = false;
        });
      }
    }
  }

  Future<void> _stop({bool cancel = false}) async {
    if (_stopping) return;
    _stopping = true;
    _ticker?.cancel();
    String? path;
    try {
      path = await _recorder.stop();
    } catch (_) {}
    final result = path ?? _path;
    if (cancel && result != null) {
      try {
        await File(result).delete();
      } catch (_) {}
    }
    if (mounted) Navigator.of(context).pop(cancel ? null : result);
  }

  String get _timer {
    final m = (_seconds ~/ 60).toString().padLeft(2, '0');
    final s = (_seconds % 60).toString().padLeft(2, '0');
    return '$m:$s';
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Dialog(
      backgroundColor: const Color(UiTokens.darkSurface),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(UiTokens.radiusCard),
      ),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              context.l10n.chatAudioNote,
              style: const TextStyle(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 12),
            if (_error != null)
              Text(_error!, style: const TextStyle(color: Colors.redAccent))
            else
              Container(
                width: 120,
                height: 120,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: scheme.primary.withAlpha(30),
                  border: Border.all(color: scheme.primary.withAlpha(90)),
                ),
                child: Icon(Icons.graphic_eq_rounded,
                    color: scheme.primary, size: 46),
              ),
            const SizedBox(height: 12),
            Text(
              _starting ? context.l10n.callConnecting : _timer,
              style: TextStyle(
                color: scheme.onSurfaceVariant,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
            const SizedBox(height: 12),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                TextButton(
                  onPressed: _stopping ? null : () => _stop(cancel: true),
                  child: Text(context.l10n.cancel),
                ),
                FilledButton.icon(
                  onPressed: _stopping || _starting ? null : _stop,
                  icon: const Icon(Icons.stop_rounded),
                  label: Text(context.l10n.chatSendNote),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
