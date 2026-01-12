/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:media_kit/media_kit.dart';
import 'package:media_kit_video/media_kit_video.dart';

class FediMediaPlayer extends StatefulWidget {
  const FediMediaPlayer({
    super.key,
    required this.url,
    this.autoplay = false,
    this.loop = false,
    this.fit = BoxFit.contain,
    this.showControls = true,
    this.muted = false,
  });

  final String url;
  final bool autoplay;
  final bool loop;
  final BoxFit fit;
  final bool showControls;
  final bool muted;

  @override
  State<FediMediaPlayer> createState() => _FediMediaPlayerState();
}

class _FediMediaPlayerState extends State<FediMediaPlayer> {
  late final Player _player = Player();
  late final VideoController _controller = VideoController(_player);

  StreamSubscription<bool>? _playingSub;
  bool _playing = false;

  @override
  void initState() {
    super.initState();
    _playingSub = _player.stream.playing.listen((v) {
      if (!mounted) return;
      setState(() => _playing = v);
    });
    _open();
  }

  @override
  void didUpdateWidget(covariant FediMediaPlayer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.url.trim() != widget.url.trim()) {
      _open();
    }
    if (oldWidget.muted != widget.muted) {
      _applyVolume();
    }
  }

  Future<void> _open() async {
    final u = widget.url.trim();
    if (u.isEmpty) return;
    try {
      await _player.setPlaylistMode(widget.loop ? PlaylistMode.single : PlaylistMode.none);
      await _player.open(Media(u), play: widget.autoplay);
      await _applyVolume();
    } catch (_) {
      // Best-effort: errors will reflect as no playback.
    }
  }

  Future<void> _applyVolume() async {
    try {
      await _player.setVolume(widget.muted ? 0 : 100);
    } catch (_) {
      // Best-effort.
    }
  }

  @override
  void dispose() {
    _playingSub?.cancel();
    _player.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        Video(
          controller: _controller,
          fit: widget.fit,
          fill: Colors.black,
          controls: widget.showControls ? AdaptiveVideoControls : null,
        ),
        if (!widget.showControls)
          Align(
            alignment: Alignment.center,
            child: IconButton(
              iconSize: 56,
              color: Colors.white,
              icon: Icon(_playing ? Icons.pause_circle : Icons.play_circle),
              onPressed: () {
                if (_playing) {
                  _player.pause();
                } else {
                  _player.play();
                }
              },
            ),
          ),
      ],
    );
  }
}
