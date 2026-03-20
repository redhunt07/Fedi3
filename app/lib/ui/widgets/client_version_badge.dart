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
  bool _hovering = false;

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
            final opacity = hasUpdate
                ? (_hovering ? 0.98 : 0.9)
                : (_hovering ? 0.72 : 0.55);
            return Material(
              elevation: 2,
              color: Theme.of(context).colorScheme.surfaceContainerHigh,
              borderRadius: BorderRadius.circular(999),
              child: MouseRegion(
                onEnter: (_) => setState(() => _hovering = true),
                onExit: (_) => setState(() => _hovering = false),
                child: AnimatedOpacity(
                  duration: const Duration(milliseconds: 180),
                  opacity: opacity,
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
                            message: _busy
                                ? 'Update in corso...'
                                : context.l10n.updateAvailable(info.version),
                            child: InkWell(
                              onTap: _busy ? null : () => _runUpdate(info),
                              borderRadius: BorderRadius.circular(999),
                              child: Padding(
                                padding: const EdgeInsets.symmetric(
                                    horizontal: 6, vertical: 2),
                                child: _busy
                                    ? Icon(
                                        Icons.lock_clock,
                                        size: 14,
                                        color: Theme.of(context)
                                            .colorScheme
                                            .primary,
                                      )
                                    : Icon(
                                        Icons.system_update_alt,
                                        size: 14,
                                        color: Theme.of(context)
                                            .colorScheme
                                            .primary
                                            .withAlpha(220),
                                      ),
                              ),
                            ),
                          ),
                        ],
                      ],
                    ),
                  ),
                ),
              ),
            );
          },
        );
      },
    );
  }

  Future<void> _runUpdate(UpdateInfo info) async {
    if (_busy) return;
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
