/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import '../../model/core_config.dart';

String resolveLocalMediaUrl(CoreConfig? cfg, String url) {
  final raw = url.trim();
  if (cfg == null || raw.isEmpty) return raw;
  final base = cfg.publicBaseUrl.trim();
  if (base.isEmpty) return raw;
  final baseTrim = base.replaceAll(RegExp(r"/+$"), "");
  final prefix = '$baseTrim/users/${cfg.username}/media/';
  if (!raw.startsWith(prefix)) return raw;
  final localBase = cfg.localBaseUri.toString().replaceAll(RegExp(r"/+$"), "");
  return '$localBase/users/${cfg.username}/media/${raw.substring(prefix.length)}';
}
