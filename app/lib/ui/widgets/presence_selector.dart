/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:material_ui/material_ui.dart';

import '../../l10n/l10n_ext.dart';
import '../../state/peer_presence_store.dart';
import '../theme/ui_tokens.dart';

/// Shared presence control used by settings and the shell. Keeping the
/// selector in one place prevents the status UI from drifting into a flat
/// segmented control on one screen and a plain popup on another.
class PresenceSelector extends StatelessWidget {
  const PresenceSelector({
    super.key,
    required this.selected,
    required this.onChanged,
    this.compact = false,
  });

  final PresenceStatus selected;
  final ValueChanged<PresenceStatus> onChanged;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        // Keep the four states on one line even on narrow phones. The pills
        // share the available width and their content scales down instead of
        // wrapping the last option onto a second row.
        final tight = compact || constraints.maxWidth < 460;
        const statuses = PresenceStatus.values;
        return Row(
          children: [
            for (var i = 0; i < statuses.length; i++) ...[
              if (i > 0) SizedBox(width: tight ? 4 : 6),
              Expanded(
                child: _PresencePill(
                  status: statuses[i],
                  selected: statuses[i] == selected,
                  compact: tight,
                  onTap: () => onChanged(statuses[i]),
                ),
              ),
            ],
          ],
        );
      },
    );
  }
}

class _PresencePill extends StatelessWidget {
  const _PresencePill({
    required this.status,
    required this.selected,
    required this.compact,
    required this.onTap,
  });

  final PresenceStatus status;
  final bool selected;
  final bool compact;
  final VoidCallback onTap;

  String _label(BuildContext context) {
    return switch (status) {
      PresenceStatus.online => context.l10n.presenceOnline,
      PresenceStatus.busy => context.l10n.presenceBusy,
      PresenceStatus.afk => context.l10n.presenceAfk,
      PresenceStatus.offline => context.l10n.presenceOffline,
    };
  }

  Color _color() {
    return switch (status) {
      PresenceStatus.online => const Color(0xFF3DDC97),
      PresenceStatus.busy => const Color(0xFFFF5A6E),
      PresenceStatus.afk => const Color(0xFFF2A33A),
      PresenceStatus.offline => const Color(0xFF8B93A5),
    };
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final dot = _color();
    final radius = BorderRadius.circular(UiTokens.radiusPill);
    final foreground = selected ? const Color(0xFF07121A) : scheme.onSurface;
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: radius,
        onTap: onTap,
        child: AnimatedContainer(
          duration: UiTokens.motionFast,
          curve: Curves.easeOutCubic,
          padding: EdgeInsets.symmetric(
            horizontal: compact ? 10 : 12,
            vertical: compact ? 6 : 8,
          ),
          decoration: BoxDecoration(
            borderRadius: radius,
            gradient: selected
                ? const LinearGradient(
                    begin: Alignment.topLeft,
                    end: Alignment.bottomRight,
                    colors: [
                      Color(UiTokens.brandCyan),
                      Color(0xFF38BDF8),
                    ],
                  )
                : null,
            color: selected ? null : const Color(UiTokens.darkSurfaceAlt),
            border: Border.all(
              color: selected
                  ? Colors.white.withAlpha(90)
                  : scheme.onSurface.withAlpha(28),
            ),
            boxShadow: selected
                ? [
                    BoxShadow(
                      color: const Color(UiTokens.brandCyan).withAlpha(70),
                      blurRadius: 12,
                      offset: const Offset(0, 3),
                    ),
                  ]
                : null,
          ),
          child: FittedBox(
            fit: BoxFit.scaleDown,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: compact ? 7 : 8,
                  height: compact ? 7 : 8,
                  decoration: BoxDecoration(
                    color: dot,
                    shape: BoxShape.circle,
                    border: Border.all(
                      color: selected
                          ? Colors.white.withAlpha(180)
                          : Colors.transparent,
                    ),
                  ),
                ),
                const SizedBox(width: 7),
                Text(
                  _label(context),
                  style: TextStyle(
                    color: foreground,
                    fontSize: compact ? 11 : 12,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                if (selected) ...[
                  const SizedBox(width: 6),
                  Icon(
                    Icons.check_rounded,
                    size: compact ? 12 : 14,
                    color: foreground,
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
