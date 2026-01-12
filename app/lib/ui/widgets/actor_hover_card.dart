/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../services/actor_repository.dart';
import 'status_avatar.dart';

class ActorHoverCard extends StatefulWidget {
  const ActorHoverCard({
    super.key,
    required this.actorUrl,
    required this.child,
    this.onTap,
  });

  final String actorUrl;
  final Widget child;
  final VoidCallback? onTap;

  @override
  State<ActorHoverCard> createState() => _ActorHoverCardState();
}

class _ActorHoverCardState extends State<ActorHoverCard> {
  final LayerLink _link = LayerLink();
  OverlayEntry? _entry;
  Timer? _timer;

  @override
  void dispose() {
    _timer?.cancel();
    _remove();
    super.dispose();
  }

  void _scheduleShow() {
    _timer?.cancel();
    _timer = Timer(const Duration(milliseconds: 250), _show);
  }

  void _show() {
    if (!mounted) return;
    if (_entry != null) return;
    final actorUrl = widget.actorUrl.trim();
    if (actorUrl.isEmpty) return;

    _entry = OverlayEntry(
      builder: (context) => Positioned.fill(
        child: Stack(
          children: [
            // Click outside to dismiss (desktop-like).
            Positioned.fill(
              child: GestureDetector(
                behavior: HitTestBehavior.translucent,
                onTap: _remove,
                child: const SizedBox.shrink(),
              ),
            ),
            CompositedTransformFollower(
              link: _link,
              targetAnchor: Alignment.bottomLeft,
              followerAnchor: Alignment.topLeft,
              offset: const Offset(0, 6),
              showWhenUnlinked: false,
              child: Material(
                color: Colors.transparent,
                child: _HoverCard(actorUrl: actorUrl, onTap: widget.onTap),
              ),
            ),
          ],
        ),
      ),
    );

    Overlay.of(context, rootOverlay: true).insert(_entry!);
  }

  void _remove() {
    _timer?.cancel();
    _timer = null;
    _entry?.remove();
    _entry = null;
  }

  @override
  Widget build(BuildContext context) {
    return CompositedTransformTarget(
      link: _link,
      child: MouseRegion(
        onEnter: (_) => _scheduleShow(),
        onExit: (_) => _remove(),
        child: GestureDetector(
          onTap: widget.onTap,
          child: widget.child,
        ),
      ),
    );
  }
}

class _HoverCard extends StatelessWidget {
  const _HoverCard({required this.actorUrl, this.onTap});

  final String actorUrl;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final host = Uri.tryParse(actorUrl)?.host ?? '';

    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 320),
      child: Card(
        elevation: 6,
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: FutureBuilder<ActorProfile?>(
            future: ActorRepository.instance.getActor(actorUrl),
            builder: (context, snap) {
              final p = snap.data;
              final iconUrl = (p?.iconUrl ?? '').trim();
              final display = (p?.displayName ?? '').trim();
              final username = (p?.preferredUsername ?? '').trim();
              final handle = (username.isNotEmpty && host.isNotEmpty) ? '@$username@$host' : actorUrl;
              final summary = (p?.summary ?? '').trim();

              return InkWell(
                onTap: onTap,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        _SmallAvatar(
                          url: iconUrl,
                          size: 40,
                          showStatus: p?.isFedi3 == true,
                          statusKey: p?.statusKey,
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                display.isNotEmpty ? display : handle,
                                style: const TextStyle(fontWeight: FontWeight.w800),
                                overflow: TextOverflow.ellipsis,
                              ),
                              const SizedBox(height: 2),
                              Text(
                                handle,
                                style: TextStyle(color: theme.colorScheme.onSurface.withAlpha(170), fontSize: 12),
                                overflow: TextOverflow.ellipsis,
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                    if (summary.isNotEmpty) ...[
                      const SizedBox(height: 10),
                      Text(
                        _stripHtml(summary),
                        maxLines: 5,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(color: theme.colorScheme.onSurface.withAlpha(200), fontSize: 12),
                      ),
                    ],
                    if (snap.connectionState == ConnectionState.waiting) ...[
                      const SizedBox(height: 10),
                      const SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2)),
                    ],
                  ],
                ),
              );
            },
          ),
        ),
      ),
    );
  }

  static String _stripHtml(String s) {
    return s.replaceAll(RegExp(r'<[^>]*>'), '').replaceAll('&nbsp;', ' ').trim();
  }
}

class _SmallAvatar extends StatelessWidget {
  const _SmallAvatar({
    required this.url,
    required this.size,
    this.showStatus = false,
    this.statusKey,
  });

  final String url;
  final double size;
  final bool showStatus;
  final String? statusKey;

  @override
  Widget build(BuildContext context) {
    return StatusAvatar(
      imageUrl: url,
      size: size,
      showStatus: showStatus,
      statusKey: statusKey,
    );
  }
}
