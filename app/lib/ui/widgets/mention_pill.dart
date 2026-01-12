/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../screens/profile_screen.dart';
import 'actor_hover_card.dart';
import 'status_avatar.dart';

class MentionPill extends StatefulWidget {
  const MentionPill({
    super.key,
    required this.appState,
    required this.actorUrl,
    required this.label,
  });

  final AppState appState;
  final String actorUrl;
  final String label;

  @override
  State<MentionPill> createState() => _MentionPillState();
}

class _MentionPillState extends State<MentionPill> {
  ActorProfile? _actor;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(covariant MentionPill oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.actorUrl != widget.actorUrl) _load();
  }

  Future<void> _load() async {
    final url = widget.actorUrl.trim();
    if (url.isEmpty) return;
    final p = await ActorRepository.instance.getActor(url);
    if (!mounted) return;
    setState(() => _actor = p);
  }

  void _openProfile() {
    final url = widget.actorUrl.trim();
    if (url.isEmpty) return;
    Navigator.of(context).push(MaterialPageRoute(builder: (_) => ProfileScreen(appState: widget.appState, actorUrl: url)));
  }

  @override
  Widget build(BuildContext context) {
    final iconUrl = (_actor?.iconUrl ?? '').trim();
    final label = widget.label.trim();

    return ActorHoverCard(
      actorUrl: widget.actorUrl,
      onTap: _openProfile,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(140),
          borderRadius: BorderRadius.circular(999),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            _TinyAvatar(
              url: iconUrl,
              size: 16,
              showStatus: _actor?.isFedi3 == true,
              statusKey: _actor?.statusKey,
            ),
            const SizedBox(width: 6),
            Text(
              label.isNotEmpty ? label : widget.actorUrl,
              style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600),
            ),
          ],
        ),
      ),
    );
  }
}

class _TinyAvatar extends StatelessWidget {
  const _TinyAvatar({
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
