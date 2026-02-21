/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../state/peer_presence_store.dart';

class StatusAvatar extends StatelessWidget {
  const StatusAvatar({
    super.key,
    required this.imageUrl,
    required this.size,
    this.showStatus = false,
    this.statusKey,
  });

  final String imageUrl;
  final double size;
  final bool showStatus;
  final String? statusKey;

  @override
  Widget build(BuildContext context) {
    final avatar = _buildAvatar(context);
    final key = statusKey?.trim().toLowerCase() ?? '';
    if (key.isEmpty) return avatar;
    return ValueListenableBuilder<Map<String, bool>>(
      valueListenable: PeerPresenceStore.instance.onlineByUsername,
      builder: (context, map, _) {
        final hasKey = map.containsKey(key);
        final online = map[key] == true;
        final statusText = online
            ? context.l10n.statusOnline
            : hasKey
                ? context.l10n.statusOffline
                : context.l10n.statusActiveRecent;
        return Tooltip(
          message: statusText,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              avatar,
              _StatusDot(size: size, online: online),
            ],
          ),
        );
      },
    );
  }

  Widget _buildAvatar(BuildContext context) {
    final u = imageUrl.trim();
    final bg = Theme.of(context).colorScheme.surfaceContainerHighest;
    if (u.isEmpty) {
      return CircleAvatar(
        radius: size / 2,
        backgroundColor: bg,
        child: Icon(Icons.person, size: (size * 0.55).clamp(16, 28)),
      );
    }
    final dpr = MediaQuery.of(context).devicePixelRatio;
    final cacheW = (size * dpr).round();
    return ClipRRect(
      borderRadius: BorderRadius.circular(size / 2),
      child: Image.network(
        u,
        width: size,
        height: size,
        fit: BoxFit.cover,
        cacheWidth: cacheW,
        filterQuality: FilterQuality.low,
        errorBuilder: (_, __, ___) => CircleAvatar(
          radius: size / 2,
          backgroundColor: bg,
          child: Icon(Icons.person, size: (size * 0.55).clamp(16, 28)),
        ),
      ),
    );
  }
}

class _StatusDot extends StatelessWidget {
  const _StatusDot({required this.size, required this.online});

  final double size;
  final bool online;

  @override
  Widget build(BuildContext context) {
    final dot = (size * 0.28).clamp(8, 12).toDouble();
    final border = Theme.of(context).colorScheme.surface;
    return Positioned(
      right: -1,
      bottom: -1,
      child: Container(
        width: dot,
        height: dot,
        decoration: BoxDecoration(
          color: online ? Colors.green : Colors.grey,
          shape: BoxShape.circle,
          border: Border.all(color: border, width: 2),
        ),
      ),
    );
  }
}
