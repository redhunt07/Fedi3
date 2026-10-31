/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:url_launcher/url_launcher.dart';

/// M-34/L-07: only these schemes may leave the app. `javascript:`, `file:`,
/// `data:` and custom schemes are ignored instead of being handed to the OS.
const Set<String> allowedExternalSchemes = {'http', 'https', 'mailto'};

bool isAllowedExternalUrl(String url) {
  final u = url.trim();
  if (u.isEmpty) return false;
  final uri = Uri.tryParse(u);
  if (uri == null) return false;
  return allowedExternalSchemes.contains(uri.scheme.toLowerCase());
}

Future<bool> openUrlExternal(String url) async {
  if (!isAllowedExternalUrl(url)) return false;
  final uri = Uri.parse(url.trim());
  try {
    return await launchUrl(uri, mode: LaunchMode.externalApplication);
  } catch (_) {
    return false;
  }
}

/// Single `onTapUrl` handler for every `HtmlWidget` in the app (M-34).
///
/// Actor/mention URLs are handled by the caller first; everything else goes
/// through the allowlist above.
class UrlPolicy {
  UrlPolicy._();

  static bool isAllowed(String url) => isAllowedExternalUrl(url);

  static FutureOr<bool> handleTap(String url) async {
    if (!isAllowedExternalUrl(url)) return false;
    return openUrlExternal(url);
  }
}
