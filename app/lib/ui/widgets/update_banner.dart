/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../services/update_service.dart';
import '../../l10n/l10n_ext.dart';
import '../utils/open_url.dart';

class UpdateBanner extends StatefulWidget {
  const UpdateBanner({super.key});

  @override
  State<UpdateBanner> createState() => _UpdateBannerState();
}

class _UpdateBannerState extends State<UpdateBanner> {
  bool _busy = false;
  String? _error;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<UpdateInfo?>(
      valueListenable: UpdateService.instance.available,
      builder: (context, info, _) {
        if (info == null) return const SizedBox.shrink();
        final l10n = context.l10n;
        return MaterialBanner(
          content: Text(l10n.updateAvailable(info.version)),
          leading: const Icon(Icons.system_update),
          actions: [
            if (_error != null)
              TextButton(
                onPressed: () => setState(() => _error = null),
                child: Text(l10n.updateDismiss),
              ),
            TextButton(
              onPressed: _busy ? null : () => _openRelease(info.releasePage),
              child: Text(l10n.updateChangelog),
            ),
            FilledButton(
              onPressed: _busy ? null : () => _apply(info),
              child: Text(_busy ? l10n.updateDownloading : l10n.updateInstall),
            ),
          ],
        );
      },
    );
  }

  Future<void> _apply(UpdateInfo info) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await UpdateService.instance.downloadAndInstall();
    } catch (e) {
      setState(() => _error = e.toString());
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.updateFailed(e.toString()))),
        );
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
      }
    }
  }

  void _openRelease(String url) {
    if (url.trim().isEmpty) return;
    openUrlExternal(url);
  }
}
