/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:url_launcher/url_launcher.dart';

Future<bool> openInAppWebView(String url) async {
  final u = url.trim();
  if (u.isEmpty) return false;
  final uri = Uri.tryParse(u);
  if (uri == null) return false;
  return launchUrl(uri, mode: LaunchMode.inAppBrowserView);
}

