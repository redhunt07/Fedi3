/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:url_launcher/url_launcher.dart';

Future<bool> openUrlExternal(String url) async {
  final u = url.trim();
  if (u.isEmpty) return false;
  final raw = Uri.tryParse(u);
  if (raw == null) return false;

  final uri = raw.hasScheme ? raw : Uri.parse('https://$u');
  if (!await canLaunchUrl(uri)) {
    return false;
  }

  if (await launchUrl(uri, mode: LaunchMode.externalApplication)) {
    return true;
  }

  return launchUrl(uri, mode: LaunchMode.platformDefault);
}
