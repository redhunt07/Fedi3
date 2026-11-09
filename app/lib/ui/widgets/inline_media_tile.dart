/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../utils/open_url.dart';
import 'fedi_media_player.dart';

class InlineMediaTile extends StatefulWidget {
  const InlineMediaTile({
    super.key,
    required this.url,
    required this.mediaType,
    required this.onOpen,
    required this.cacheWidth,
    this.name,
    this.borderRadius = 12,
    this.autoplay = false,
    this.muted = false,
    this.poster = false,
  });

  final String url;
  final String mediaType;

  /// Original attachment name, used to infer the kind when the media type is
  /// missing or a generic `application/octet-stream`.
  final String? name;
  final VoidCallback onOpen;
  final int cacheWidth;
  final double borderRadius;
  final bool autoplay;
  final bool muted;

  /// Renders the first video frame behind the play overlay so a video message
  /// shows a real poster instead of a black rectangle. Opt-in because every
  /// poster keeps a player instance alive.
  final bool poster;

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

  bool get _isVideoOrAudio => _isAudio || _isVideo;

  bool get _isGenericMediaType {
    final mt = widget.mediaType.toLowerCase().trim();
    return mt.isEmpty ||
        mt == 'application/octet-stream' ||
        mt == 'binary/octet-stream';
  }

  bool _hasExtension(String? value, Set<String> extensions) {
    var v = (value ?? '').toLowerCase().trim();
    if (v.isEmpty) return false;
    final query = v.indexOf('?');
    if (query >= 0) v = v.substring(0, query);
    final fragment = v.indexOf('#');
    if (fragment >= 0) v = v.substring(0, fragment);
    return extensions.any(v.endsWith);
  }

  static const Set<String> _audioExtensions = {
    '.m4a',
    '.mp3',
    '.ogg',
    '.opus',
    '.wav',
    '.aac',
    '.flac',
    '.oga',
  };

  static const Set<String> _videoExtensions = {
    '.mp4',
    '.webm',
    '.mov',
    '.m4v',
    '.mkv',
    '.ogv',
  };

  bool get _isAudio {
    final mt = widget.mediaType.toLowerCase().trim();
    if (mt.startsWith('audio/')) return true;
    if (mt.startsWith('video/')) return false;
    if (!_isGenericMediaType) return false;
    return _hasExtension(widget.url, _audioExtensions) ||
        _hasExtension(widget.name, _audioExtensions);
  }

  bool get _isVideo {
    final mt = widget.mediaType.toLowerCase().trim();
    if (mt.startsWith('video/')) return true;
    if (mt.startsWith('audio/')) return false;
    if (!_isGenericMediaType) return false;
    return _hasExtension(widget.url, _videoExtensions) ||
        _hasExtension(widget.name, _videoExtensions);
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
            semanticLabel: context.l10n.semanticsMedia,
            fit: BoxFit.cover,
            cacheWidth: widget.cacheWidth,
            filterQuality: FilterQuality.low,
            loadingBuilder: (context, child, progress) {
              if (progress == null) return child;
              return Container(
                color: Theme.of(context).colorScheme.surfaceContainerHighest,
                alignment: Alignment.center,
                child: const SizedBox(
                    width: 26,
                    height: 26,
                    child: CircularProgressIndicator(strokeWidth: 2)),
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

    final isAudio = _isAudio;
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
              Stack(
                fit: StackFit.expand,
                children: [
                  if (widget.poster && !isAudio)
                    FediMediaPlayer(
                      url: url,
                      autoplay: false,
                      fit: BoxFit.cover,
                      showControls: false,
                      muted: true,
                    ),
                  InkWell(
                    onTap: () => setState(() => _showPlayer = true),
                    child: Container(
                      color: widget.poster && !isAudio
                          ? Colors.black.withAlpha(60)
                          : Colors.black.withAlpha(210),
                      alignment: Alignment.center,
                      child: Icon(
                          isAudio ? Icons.audiotrack : Icons.play_circle_fill,
                          color: Colors.white,
                          size: 46),
                    ),
                  ),
                ],
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
