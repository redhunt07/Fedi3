/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';
import 'backup_screen.dart';
import 'dev_core_screen.dart';
import 'edit_config_screen.dart';
import 'moderation_settings_screen.dart';
import 'networking_settings_screen.dart';
import 'privacy_settings_screen.dart';
import 'profile_edit_screen.dart';
import 'security_settings_screen.dart';
import 'translation_settings_screen.dart';
import 'ui_settings_screen.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final cfg = widget.appState.config!;

        return Scaffold(
          appBar: AppBar(title: Text(context.l10n.settingsTitle)),
          body: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Card(
                child: ListTile(
                  title: Text(context.l10n.settingsCore),
                  subtitle: Text(
                    widget.appState.isRunning
                        ? context.l10n.settingsCoreRunning(widget.appState.coreHandle ?? 0)
                        : context.l10n.settingsCoreStopped,
                  ),
                  trailing: FilledButton(
                    onPressed: () async {
                      if (widget.appState.isRunning) {
                        await widget.appState.stopCore();
                      } else {
                        await widget.appState.startCore();
                      }
                    },
                    child: Text(widget.appState.isRunning ? context.l10n.coreStop : context.l10n.coreStart),
                  ),
                ),
              ),
              if (widget.appState.lastError != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    widget.appState.lastError!,
                    style: TextStyle(color: Theme.of(context).colorScheme.error),
                  ),
                ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.profileEditTitle),
                  subtitle: Text(context.l10n.profileEditHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => ProfileEditScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.privacyTitle),
                  subtitle: Text(context.l10n.privacyHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => PrivacySettingsScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.securityTitle),
                  subtitle: Text(context.l10n.securityHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => SecuritySettingsScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.moderationTitle),
                  subtitle: Text(context.l10n.moderationHintTitle),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => ModerationSettingsScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.networkingTitle),
                  subtitle: Text(context.l10n.networkingHintTitle),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => NetworkingSettingsScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.backupTitle),
                  subtitle: Text(context.l10n.backupHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => BackupScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.uiSettingsTitle),
                  subtitle: Text(context.l10n.uiSettingsHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => UiSettingsScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.translationTitle),
                  subtitle: Text(context.l10n.translationHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => TranslationSettingsScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.settingsAccount),
                  subtitle: Text('${cfg.username}@${cfg.domain}'),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => EditConfigScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 16),
              Card(
                child: ListTile(
                  title: Text(context.l10n.settingsAdvancedDev),
                  subtitle: Text(context.l10n.settingsAdvancedDevHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => DevCoreScreen(appState: widget.appState),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 16),
              OutlinedButton(
                onPressed: () async {
                  await widget.appState.stopCore();
                  await widget.appState.clearConfig();
                },
                child: Text(context.l10n.settingsResetApp),
              ),
            ],
          ),
        );
      },
    );
  }
}
