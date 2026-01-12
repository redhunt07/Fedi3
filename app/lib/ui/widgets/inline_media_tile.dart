/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../utils/open_url.dart';
import 'fedi_media_player.dart';

class InlineMediaTile extends StatefulWidget {
  const InlineMediaTile({
    super.key,
    required this.url,
    required this.mediaType,
    required this.onOpen,
    required this.cacheWidth,
    this.borderRadius = 12,
    this.autoplay = false,
    this.muted = false,
  });

  final String url;
  final String mediaType;
  final VoidCallback onOpen;
  final int cacheWidth;
  final double borderRadius;
  final bool autoplay;
  final bool muted;

  @override
  State<InlineMediaTile> createState() => _InlineMediaTileState();
}

class _InlineMediaTileState extends State<InlineMediaTile> {
  bool _precached = false;
  bool _showPlayer = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_precached) return;
    final url = widget.url.trim();
    if (url.isNotEmpty && !_isVideoOrAudio) {
      precacheImage(NetworkImage(url), context);
      _precached = true;
    }
  }

  bool get _isVideoOrAudio {
    final mt = widget.mediaType.toLowerCase();
    if (mt.startsWith('video/') || mt.startsWith('audio/')) return true;
    if (mt.isNotEmpty) return false;
    final u = widget.url.toLowerCase();
    return u.endsWith('.mp4') ||
        u.endsWith('.webm') ||
        u.endsWith('.mov') ||
        u.endsWith('.m4v') ||
        u.endsWith('.mp3') ||
        u.endsWith('.ogg') ||
        u.endsWith('.wav') ||
        u.endsWith('.flac') ||
        u.endsWith('.m4a') ||
        u.endsWith('.opus');
  }

  @override
  Widget build(BuildContext context) {
    final url = widget.url.trim();
    if (url.isEmpty) return const SizedBox.shrink();

    if (!_isVideoOrAudio) {
      return ClipRRect(
        borderRadius: BorderRadius.circular(widget.borderRadius),
        child: InkWell(
          onTap: widget.onOpen,
          child: Image.network(
            url,
            fit: BoxFit.cover,
            cacheWidth: widget.cacheWidth,
            filterQuality: FilterQuality.low,
            loadingBuilder: (context, child, progress) {
              if (progress == null) return child;
              return Container(
                color: Theme.of(context).colorScheme.surfaceContainerHighest,
                alignment: Alignment.center,
                child: const SizedBox(width: 26, height: 26, child: CircularProgressIndicator(strokeWidth: 2)),
              );
            },
            errorBuilder: (_, __, ___) => Container(
              color: Theme.of(context).colorScheme.surfaceContainerHighest,
              child: const Icon(Icons.broken_image_outlined),
            ),
          ),
        ),
      );
    }

    final isAudio = widget.mediaType.toLowerCase().startsWith('audio/');
    return ClipRRect(
      borderRadius: BorderRadius.circular(widget.borderRadius),
      child: AspectRatio(
        aspectRatio: 16 / 9,
        child: Stack(
          fit: StackFit.expand,
          children: [
            if (_showPlayer)
              FediMediaPlayer(
                url: url,
                autoplay: true,
                loop: false,
                fit: BoxFit.cover,
                showControls: true,
                muted: widget.muted,
              )
            else
              InkWell(
                onTap: () => setState(() => _showPlayer = true),
                child: Container(
                  color: Colors.black.withAlpha(210),
                  alignment: Alignment.center,
                  child: Icon(isAudio ? Icons.audiotrack : Icons.play_circle_fill, color: Colors.white, size: 46),
                ),
              ),
            Align(
              alignment: Alignment.topRight,
              child: Padding(
                padding: const EdgeInsets.all(6),
                child: IconButton(
                  tooltip: 'Open',
                  onPressed: widget.onOpen,
                  icon: const Icon(Icons.open_in_full, color: Colors.white),
                ),
              ),
            ),
            Align(
              alignment: Alignment.topLeft,
              child: Padding(
                padding: const EdgeInsets.all(6),
                child: IconButton(
                  tooltip: 'Open in browser',
                  onPressed: () => openUrlExternal(url),
                  icon: const Icon(Icons.open_in_new, color: Colors.white),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
