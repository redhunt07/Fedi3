/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:material_ui/material_ui.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/background_service.dart';

/// Android-only toggle for the foreground keep-alive service that keeps
/// notifications flowing while the app is in the background.
class BackgroundServiceSettings extends StatefulWidget {
  const BackgroundServiceSettings({super.key});

  @override
  State<BackgroundServiceSettings> createState() =>
      _BackgroundServiceSettingsState();
}

class _BackgroundServiceSettingsState extends State<BackgroundServiceSettings> {
  bool _enabled = true;
  bool _running = false;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final enabled = await BackgroundService.isEnabled();
    final running = await BackgroundService.isRunning();
    if (!mounted) return;
    setState(() {
      _enabled = enabled;
      _running = running;
    });
  }

  Future<void> _change(bool value) async {
    setState(() => _busy = true);
    final enabled = await BackgroundService.setEnabled(
      value,
      statusText: context.l10n.backgroundServiceNotificationBody,
    );
    final running = await BackgroundService.isRunning();
    if (!mounted) return;
    setState(() {
      _enabled = enabled;
      _running = running;
      _busy = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Card(
      child: Column(
        children: [
          SwitchListTile(
            title: Text(context.l10n.backgroundServiceTitle),
            subtitle: Text(context.l10n.backgroundServiceHint),
            value: _enabled,
            onChanged: _busy ? null : _change,
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
            child: Row(
              children: [
                Icon(
                  _running ? Icons.sync_rounded : Icons.pause_circle_outline,
                  size: 16,
                  color: _running ? scheme.primary : scheme.onSurfaceVariant,
                ),
                const SizedBox(width: 6),
                Text(
                  _running
                      ? context.l10n.backgroundServiceRunning
                      : context.l10n.backgroundServiceIdle,
                  style: TextStyle(
                    fontSize: 12,
                    color: _running ? scheme.primary : scheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
