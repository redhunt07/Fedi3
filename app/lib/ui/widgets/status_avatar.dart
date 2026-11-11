/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:material_ui/material_ui.dart';

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
    final key = statusKey?.trim() ?? '';
    if (!showStatus || key.isEmpty) return avatar;
    return ValueListenableBuilder<Map<String, PresenceStatus>>(
      valueListenable: PeerPresenceStore.instance.statusByUsername,
      builder: (context, map, _) {
        final status = map[key] ?? PresenceStatus.offline;
        return Stack(
          clipBehavior: Clip.none,
          children: [
            avatar,
            _StatusDot(size: size, status: status),
          ],
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
        semanticLabel: context.l10n.semanticsAvatar,
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
  const _StatusDot({required this.size, required this.status});

  final double size;
  final PresenceStatus status;

  @override
  Widget build(BuildContext context) {
    final dot = (size * 0.28).clamp(8, 12).toDouble();
    final border = Theme.of(context).colorScheme.surface;
    final color = switch (status) {
      PresenceStatus.online => const Color(0xFF3DDC97),
      PresenceStatus.busy => const Color(0xFFFF5A6E),
      PresenceStatus.afk => const Color(0xFFF2A33A),
      PresenceStatus.offline => const Color(0xFF8B93A5),
    };
    final label = switch (status) {
      PresenceStatus.online => context.l10n.presenceOnline,
      PresenceStatus.busy => context.l10n.presenceBusy,
      PresenceStatus.afk => context.l10n.presenceAfk,
      PresenceStatus.offline => context.l10n.presenceOffline,
    };
    return Positioned(
      right: -1,
      bottom: -1,
      child: Semantics(
        label: label,
        child: Container(
          width: dot,
          height: dot,
          decoration: BoxDecoration(
            color: color,
            shape: BoxShape.circle,
            border: Border.all(color: border, width: 2),
            boxShadow: status == PresenceStatus.online
                ? [
                    BoxShadow(
                      color: color.withAlpha(140),
                      blurRadius: 6,
                    ),
                  ]
                : null,
          ),
        ),
      ),
    );
  }
}
