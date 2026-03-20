/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/update_service.dart';

class ClientVersionBadge extends StatefulWidget {
  const ClientVersionBadge({super.key});

  @override
  State<ClientVersionBadge> createState() => _ClientVersionBadgeState();
}

class _ClientVersionBadgeState extends State<ClientVersionBadge> {
  late final Future<String> _versionFuture = _loadVersion();
  bool _busy = false;

  Future<String> _loadVersion() async {
    final pkg = await PackageInfo.fromPlatform();
    final v = pkg.version.trim();
    final b = pkg.buildNumber.trim();
    return b.isEmpty ? v : '$v+$b';
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<String>(
      future: _versionFuture,
      builder: (context, snap) {
        final version = snap.data?.trim() ?? '-';
        return ValueListenableBuilder<UpdateInfo?>(
          valueListenable: UpdateService.instance.available,
          builder: (context, info, _) {
            final hasUpdate = info != null;
            return Material(
              elevation: 2,
              color: Theme.of(context).colorScheme.surfaceContainerHigh,
              borderRadius: BorderRadius.circular(999),
              child: Padding(
                padding:
                    const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      'client v$version',
                      style: const TextStyle(
                          fontSize: 12, fontWeight: FontWeight.w600),
                    ),
                    if (hasUpdate) ...[
                      const SizedBox(width: 8),
                      Tooltip(
                        message: context.l10n.updateAvailable(info.version),
                        child: InkWell(
                          onTap: _busy ? null : () => _runUpdate(info),
                          borderRadius: BorderRadius.circular(999),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 6, vertical: 2),
                            child: _busy
                                ? const SizedBox(
                                    width: 14,
                                    height: 14,
                                    child: CircularProgressIndicator(
                                        strokeWidth: 2),
                                  )
                                : Icon(
                                    Icons.system_update_alt,
                                    size: 16,
                                    color:
                                        Theme.of(context).colorScheme.primary,
                                  ),
                          ),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            );
          },
        );
      },
    );
  }

  Future<void> _runUpdate(UpdateInfo info) async {
    setState(() => _busy = true);
    try {
      await UpdateService.instance.launchManualUpdateAndExit(info: info);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.updateFailed(e.toString()))),
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }
}
