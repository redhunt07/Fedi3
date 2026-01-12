/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_selector/file_selector.dart';
import 'package:http/http.dart' as http;
import 'package:share_plus/share_plus.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/note_models.dart';
import '../../state/app_state.dart';
import '../utils/open_url.dart';
import '../utils/media_url.dart';
import '../widgets/fedi_media_player.dart';

class MediaViewerScreen extends StatefulWidget {
  const MediaViewerScreen({
    super.key,
    required this.appState,
    required this.url,
    required this.mediaType,
    this.attachments = const [],
    this.initialIndex = 0,
  });

  final AppState appState;
  final String url;
  final String mediaType;
  final List<NoteAttachment> attachments;
  final int initialIndex;

  @override
  State<MediaViewerScreen> createState() => _MediaViewerScreenState();
}

class _MediaViewerScreenState extends State<MediaViewerScreen> {
  late final PageController _pager = PageController(initialPage: widget.initialIndex);
  late List<NoteAttachment> _items;
  late List<NoteAttachment> _resolvedItems;
  late bool _autoplay;
  late bool _muted;
  int _index = 0;

  @override
  void initState() {
    super.initState();
    _items = widget.attachments.isNotEmpty ? widget.attachments : [NoteAttachment(url: widget.url, mediaType: widget.mediaType)];
    _resolvedItems = _items
        .map(
          (a) => NoteAttachment(
            url: resolveLocalMediaUrl(widget.appState.config, a.url),
            mediaType: a.mediaType,
          ),
        )
        .toList();
    _index = widget.initialIndex.clamp(0, _items.length - 1);
    _autoplay = widget.appState.prefs.mediaAutoplay;
    _muted = widget.appState.prefs.mediaMuted;
    _precacheAround(_index);
  }

  @override
  void dispose() {
    _pager.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final current = _items[_index];

    return Scaffold(
      appBar: AppBar(
        backgroundColor: Colors.black,
        foregroundColor: Colors.white,
        actions: [
          IconButton(
            tooltip: context.l10n.copy,
            onPressed: () async {
              await Clipboard.setData(ClipboardData(text: current.url));
              if (!context.mounted) return;
              ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.copied)));
            },
            icon: const Icon(Icons.content_copy),
          ),
          IconButton(
            tooltip: 'Download',
            onPressed: () => _downloadCurrent(current),
            icon: const Icon(Icons.download),
          ),
          IconButton(
            tooltip: 'Share',
            onPressed: () => Share.share(current.url),
            icon: const Icon(Icons.share),
          ),
          IconButton(
            tooltip: _autoplay ? 'Autoplay on' : 'Autoplay off',
            onPressed: () async {
              final next = !_autoplay;
              setState(() => _autoplay = next);
              await widget.appState.savePrefs(widget.appState.prefs.copyWith(mediaAutoplay: next));
            },
            icon: Icon(_autoplay ? Icons.play_circle : Icons.play_circle_outline),
          ),
          IconButton(
            tooltip: _muted ? 'Muted' : 'Sound on',
            onPressed: () async {
              final next = !_muted;
              setState(() => _muted = next);
              await widget.appState.savePrefs(widget.appState.prefs.copyWith(mediaMuted: next));
            },
            icon: Icon(_muted ? Icons.volume_off : Icons.volume_up),
          ),
          IconButton(
            tooltip: 'Open in browser',
            onPressed: () => openUrlExternal(current.url),
            icon: const Icon(Icons.open_in_new),
          ),
        ],
      ),
      backgroundColor: Colors.black,
      body: Center(
        child: PageView.builder(
          controller: _pager,
          itemCount: _items.length,
          onPageChanged: (idx) {
            setState(() => _index = idx);
            _precacheAround(idx);
          },
          itemBuilder: (context, index) {
            final item = _resolvedItems[index];
            final mt = item.mediaType.toLowerCase();
            final isVideo = mt.startsWith('video/');
            final isAudio = mt.startsWith('audio/');
            final isImage = mt.startsWith('image/') || (!isVideo && !isAudio);
            if (isImage) {
              return InteractiveViewer(
                minScale: 0.5,
                maxScale: 5,
                child: Image.network(
                  item.url,
                  fit: BoxFit.contain,
                  loadingBuilder: (context, child, progress) {
                    if (progress == null) return child;
                    return const Center(child: CircularProgressIndicator());
                  },
                  errorBuilder: (_, __, ___) => const Center(child: Icon(Icons.broken_image_outlined, color: Colors.white)),
                ),
              );
            }
            return Stack(
              children: [
                Positioned.fill(
                  child: FediMediaPlayer(
                    url: item.url,
                    autoplay: _autoplay,
                    loop: false,
                    fit: BoxFit.contain,
                    showControls: true,
                    muted: _muted,
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }

  Future<void> _downloadCurrent(NoteAttachment item) async {
    final url = resolveLocalMediaUrl(widget.appState.config, item.url).trim();
    if (url.isEmpty) return;
    final parts = url.split('/').where((s) => s.isNotEmpty).toList();
    final name = parts.isNotEmpty ? parts.last : 'media';
    final loc = await getSaveLocation(suggestedName: name);
    final path = loc?.path;
    if (path == null || path.isEmpty) return;
    try {
      final resp = await http.get(Uri.parse(url));
      if (resp.statusCode < 200 || resp.statusCode >= 300) {
        throw StateError('download failed: ${resp.statusCode}');
      }
      final file = XFile.fromData(resp.bodyBytes, mimeType: item.mediaType, name: name);
      await file.saveTo(path);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Downloaded')));
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Download failed: $e')));
    }
  }

  void _precacheAround(int idx) {
    for (final i in [idx - 1, idx + 1]) {
      if (i < 0 || i >= _items.length) continue;
      final item = _resolvedItems[i];
      final mt = item.mediaType.toLowerCase();
      final isImage = mt.startsWith('image/') || (!mt.startsWith('video/') && !mt.startsWith('audio/'));
      if (isImage) {
        precacheImage(NetworkImage(item.url), context);
      }
    }
  }
}
