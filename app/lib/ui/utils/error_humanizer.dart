/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';

String humanizeError(BuildContext context, String raw) {
  final lower = raw.toLowerCase();
  if (lower.contains('unable to load fedi3_core library') || lower.contains('libfedi3_core')) {
    return context.l10n.errorCoreLibraryMissing;
  }
  if (lower.contains('relay_token missing/too short') || lower.contains('token too short')) {
    return context.l10n.errorRelayTokenTooShort;
  }
  if (lower.contains('relay_ws must start') || lower.contains('relay ws must start')) {
    return context.l10n.errorRelayWsInvalid;
  }
  if (lower.contains('admin token required')) {
    return context.l10n.errorRelayRegistrationDisabled;
  }
  if (lower.contains('connection refused') ||
      lower.contains('connection failed') ||
      lower.contains('connection error') ||
      lower.contains('failed host lookup')) {
    return context.l10n.errorRelayUnreachable;
  }
  return raw;
}
