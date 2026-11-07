/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/relay_directory_service.dart';

/// Human-readable label for a relay latency probe.
String relayLatencyLabel(BuildContext context, RelayLatency latency) {
  if (latency.reachable) {
    return context.l10n.relayLatencyMs(latency.latencyMs ?? 0);
  }
  switch (latency.failure) {
    case RelayProbeFailure.tlsHostMismatch:
      return context.l10n.relayLatencyTlsMismatch;
    case RelayProbeFailure.tlsCertificate:
      return context.l10n.relayLatencyTlsCertificate;
    case RelayProbeFailure.timeout:
      return context.l10n.relayLatencyTimeout;
    case RelayProbeFailure.httpStatus:
      return context.l10n.relayLatencyHttpError;
    case RelayProbeFailure.network:
    case null:
      return context.l10n.relayLatencyNetworkError;
  }
}

/// Compact latency/status chip shown next to a relay in rankings.
class RelayLatencyBadge extends StatelessWidget {
  const RelayLatencyBadge(
      {super.key, required this.latency, this.compact = true});

  final RelayLatency latency;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final ms = latency.latencyMs;
    final Color color;
    if (latency.reachable) {
      if (ms! <= 120) {
        color = const Color(0xFF2ECC71);
      } else if (ms <= 350) {
        color = const Color(0xFFF5A524);
      } else {
        color = const Color(0xFFF97316);
      }
    } else if (latency.failure == RelayProbeFailure.tlsHostMismatch ||
        latency.failure == RelayProbeFailure.tlsCertificate) {
      color = scheme.error;
    } else {
      color = scheme.onSurfaceVariant;
    }
    final text = relayLatencyLabel(context, latency);
    return Container(
      padding: EdgeInsets.symmetric(
          horizontal: compact ? 8 : 10, vertical: compact ? 3 : 4),
      decoration: BoxDecoration(
        color: color.withAlpha(28),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: color.withAlpha(90)),
      ),
      child: Text(
        text,
        style: TextStyle(
          color: color,
          fontSize: compact ? 11 : 12,
          fontWeight: FontWeight.w700,
        ),
      ),
    );
  }
}
