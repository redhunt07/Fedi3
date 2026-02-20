/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:media_kit/media_kit.dart';
import 'package:media_kit_video/media_kit_video.dart';
import 'package:path_provider/path_provider.dart';

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
  late Player _player;
  late VideoController _controller;

  StreamSubscription<bool>? _playingSub;
  bool _playing = false;
  bool _loading = false;
  int _openGeneration = 0;

  @override
  void initState() {
    super.initState();
    MediaKit.ensureInitialized();
    _player = Player();
    _controller = VideoController(_player);
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
    final gen = ++_openGeneration;
    final u = widget.url.trim();
    if (u.isEmpty) return;
    try {
      if (mounted) setState(() => _loading = true);
      await _player.setPlaylistMode(widget.loop ? PlaylistMode.single : PlaylistMode.none);
      final resolved = await _resolvePlayableUrl(u);
      if (gen != _openGeneration) return;
      await _player.open(Media(resolved), play: widget.autoplay);
      await _applyVolume();
    } catch (_) {
      // Best-effort: errors will reflect as no playback.
    } finally {
      if (mounted && gen == _openGeneration) {
        setState(() => _loading = false);
      }
    }
  }

  Future<String> _resolvePlayableUrl(String url) async {
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      return url;
    }
    if (Platform.isLinux) {
      final cached = await _loadOrDownloadMedia(url);
      if (cached != null) return cached;
    }
    if (await _supportsRange(url)) {
      return url;
    }
    final cached = await _loadOrDownloadMedia(url);
    return cached ?? url;
  }

  Future<bool> _supportsRange(String url) async {
    try {
      final resp = await http.head(Uri.parse(url));
      final acceptRanges = resp.headers['accept-ranges'] ?? '';
      if (acceptRanges.toLowerCase().contains('bytes')) {
        return true;
      }
    } catch (_) {
      // Best-effort.
    }
    return false;
  }

  Future<String?> _loadOrDownloadMedia(String url) async {
    try {
      final dir = await getTemporaryDirectory();
      final fileName = _mediaCacheName(url);
      final file = File('${dir.path}${Platform.pathSeparator}$fileName');
      if (await file.exists()) {
        final len = await file.length();
        if (len > 0) return file.path;
      }
      final resp = await http.Client().send(http.Request('GET', Uri.parse(url)));
      if (resp.statusCode < 200 || resp.statusCode >= 300) {
        return null;
      }
      await file.parent.create(recursive: true);
      final sink = file.openWrite();
      await resp.stream.pipe(sink);
      await sink.flush();
      await sink.close();
      return file.path;
    } catch (_) {
      return null;
    }
  }

  String _mediaCacheName(String url) {
    final encoded = base64UrlEncode(utf8.encode(url));
    final short = encoded.length > 64 ? encoded.substring(0, 64) : encoded;
    return 'fedi_media_$short';
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
        if (_loading)
          const Align(
            alignment: Alignment.center,
            child: SizedBox(width: 28, height: 28, child: CircularProgressIndicator(strokeWidth: 2)),
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
