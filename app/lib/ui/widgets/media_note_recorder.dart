/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';
import 'package:path_provider/path_provider.dart';

import '../../l10n/l10n_ext.dart';

import '../theme/ui_tokens.dart';

/// Records a short audio or video note with the same WebRTC media stack used
/// for calls, then returns the local file path to the composer.
class MediaNoteRecorder extends StatefulWidget {
  const MediaNoteRecorder({super.key, required this.video});

  final bool video;

  @override
  State<MediaNoteRecorder> createState() => _MediaNoteRecorderState();
}

class _MediaNoteRecorderState extends State<MediaNoteRecorder> {
  MediaStream? _stream;
  MediaRecorder? _recorder;
  RTCVideoRenderer? _renderer;
  String? _path;
  String? _error;
  bool _starting = true;
  bool _stopping = false;
  Timer? _ticker;
  int _seconds = 0;

  @override
  void initState() {
    super.initState();
    _start();
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _stream?.getTracks().forEach((t) => t.stop());
    _renderer?.dispose();
    super.dispose();
  }

  Future<void> _start() async {
    try {
      if (widget.video) {
        _renderer = RTCVideoRenderer();
        await _renderer!.initialize();
      }
      _stream = await navigator.mediaDevices.getUserMedia({
        'audio': true,
        'video': widget.video
            ? {
                'facingMode': 'user',
                'width': 1280,
                'height': 720,
                'frameRate': 30
              }
            : false,
      });
      if (_renderer != null) {
        // Starting the muxer before the camera delivers a frame produces a
        // zero-byte file on some devices, so wait for the first frame first.
        final firstFrame = Completer<void>();
        _renderer!.onFirstFrameRendered = () {
          if (!firstFrame.isCompleted) firstFrame.complete();
        };
        _renderer!.srcObject = _stream;
        if (widget.video) {
          await firstFrame.future.timeout(
            const Duration(seconds: 8),
            onTimeout: () {},
          );
        }
      }
      final dir = await getTemporaryDirectory();
      _path =
          '${dir.path}/fedi3_note_${DateTime.now().microsecondsSinceEpoch}.${widget.video ? 'mp4' : 'm4a'}';
      _recorder = MediaRecorder();
      await _recorder!.start(
        _path!,
        videoTrack: widget.video && _stream!.getVideoTracks().isNotEmpty
            ? _stream!.getVideoTracks().first
            : null,
        // Video notes must keep their sound: enable the input audio channel
        // whenever the captured stream has an audio track. The first-frame wait
        // and the zero-byte guard below still protect the muxer on devices
        // where the camera is slow to deliver a frame.
        audioChannel: _stream!.getAudioTracks().isNotEmpty
            ? RecorderAudioChannel.INPUT
            : null,
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
    try {
      await _recorder?.stop();
    } catch (_) {}
    _stream?.getTracks().forEach((t) => t.stop());
    _renderer?.dispose();
    final path = _path;
    var size = 0;
    if (path != null) {
      try {
        size = await File(path).length();
      } catch (_) {}
    }
    if (cancel || size == 0) {
      if (path != null) {
        try {
          await File(path).delete();
        } catch (_) {}
      }
      if (mounted) Navigator.of(context).pop(null);
      return;
    }
    if (mounted) Navigator.of(context).pop(path);
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
              widget.video
                  ? context.l10n.chatVideoNote
                  : context.l10n.chatAudioNote,
              style: const TextStyle(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 12),
            if (_error != null)
              Text(_error!, style: const TextStyle(color: Colors.redAccent))
            else if (widget.video)
              ClipRRect(
                borderRadius: BorderRadius.circular(14),
                child: SizedBox(
                  width: 320,
                  height: 180,
                  child: _renderer != null
                      ? RTCVideoView(_renderer!,
                          mirror: true,
                          objectFit:
                              RTCVideoViewObjectFit.RTCVideoViewObjectFitCover)
                      : const Center(child: CircularProgressIndicator()),
                ),
              )
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
