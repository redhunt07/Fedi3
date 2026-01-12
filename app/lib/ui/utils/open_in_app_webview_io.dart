/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:desktop_webview_window/desktop_webview_window.dart';
import 'package:path_provider/path_provider.dart';
import 'package:url_launcher/url_launcher.dart';

String _massageForWebView(String url) {
  final u = url.trim();
  if (u.isEmpty) return u;
  final uri = Uri.tryParse(u);
  if (uri == null) return u;

  final host = uri.host.toLowerCase();
  final isYouTube = host.contains('youtube.com') || host.contains('youtube-nocookie.com') || host == 'youtu.be' || host.endsWith('.youtu.be');
  if (!isYouTube) return u;

  String? id;
  if (host == 'youtu.be' || host.endsWith('.youtu.be')) {
    if (uri.pathSegments.isNotEmpty) id = uri.pathSegments.first;
  } else {
    id = uri.queryParameters['v'];
    if ((id == null || id.isEmpty) && uri.pathSegments.length >= 2) {
      final seg = uri.pathSegments;
      final iEmbed = seg.indexOf('embed');
      if (iEmbed >= 0 && iEmbed + 1 < seg.length) id = seg[iEmbed + 1];
      final iShorts = seg.indexOf('shorts');
      if ((id == null || id.isEmpty) && iShorts >= 0 && iShorts + 1 < seg.length) id = seg[iShorts + 1];
    }
  }
  id = id?.trim();
  if (id == null || id.isEmpty) return u;

  // YouTube embeds sometimes fail in WebView2 with "player configuration error" (e.g. error 153).
  // Use the watch page instead, which is more reliable across desktop WebViews.
  return Uri.https('www.youtube.com', '/watch', {'v': id}).toString();
}

Future<bool> openInAppWebView(String url) async {
  final u = _massageForWebView(url);
  if (u.isEmpty) return false;
  final uri = Uri.tryParse(u);
  if (uri == null) return false;

  if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
    try {
      if (Platform.isLinux) {
        return launchUrl(uri, mode: LaunchMode.externalApplication);
      }
      if (Platform.isWindows && !(await WebviewWindow.isWebviewAvailable())) {
        return launchUrl(uri, mode: LaunchMode.externalApplication);
      }

      final cfg = Platform.isWindows
          ? CreateConfiguration(
              userDataFolderWindows: (await getApplicationSupportDirectory()).path,
              titleBarHeight: 44,
            )
          : const CreateConfiguration();
      final webview = await WebviewWindow.create(configuration: cfg);
      webview.launch(u);
      return true;
    } catch (_) {
      return launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  }

  return launchUrl(uri, mode: LaunchMode.inAppBrowserView);
}
