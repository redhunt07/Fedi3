/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';
import '../utils/error_humanizer.dart';

class CoreNotRunningCard extends StatelessWidget {
  const CoreNotRunningCard({
    super.key,
    required this.appState,
    this.hint,
    this.onStarted,
  });

  final AppState appState;
  final String? hint;
  final VoidCallback? onStarted;

  @override
  Widget build(BuildContext context) {
    final error = appState.lastError?.trim();
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  context.l10n.notificationsCoreNotRunning,
                  style: const TextStyle(fontWeight: FontWeight.w800),
                ),
                if (hint != null && hint!.trim().isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Text(hint!),
                ],
                const SizedBox(height: 12),
                Text(context.l10n.settingsCoreServiceHint),
                if (error != null && error.isNotEmpty) ...[
                  const SizedBox(height: 10),
                  Text(
                    humanizeError(context, error),
                    style: TextStyle(color: Theme.of(context).colorScheme.error),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
