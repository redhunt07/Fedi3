/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/widgets.dart';

import '../../l10n/l10n_ext.dart';

String formatTimeAgo(BuildContext context, DateTime when, {DateTime? now}) {
  final ref = now ?? DateTime.now();
  var diff = ref.difference(when);
  if (diff.isNegative) diff = Duration.zero;

  if (diff.inSeconds < 45) return context.l10n.timeAgoJustNow;

  final mins = diff.inMinutes;
  if (mins < 60) return context.l10n.timeAgoMinutes(mins);

  final hours = diff.inHours;
  if (hours < 24) return context.l10n.timeAgoHours(hours);

  final days = diff.inDays;
  if (days < 30) return context.l10n.timeAgoDays(days);

  final months = (days / 30).floor().clamp(1, 1200);
  if (months < 12) return context.l10n.timeAgoMonths(months);

  final years = (days / 365).floor().clamp(1, 1000);
  return context.l10n.timeAgoYears(years);
}

