/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';
import 'backup_screen.dart';
import 'dev_core_screen.dart';
import 'edit_config_screen.dart';
import 'follow_requests_screen.dart';
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
            padding: const EdgeInsets.fromLTRB(20, 16, 20, 24),
            children: [
              _accountHeader(context, cfg),
              if (widget.appState.lastError != null)
                Padding(
                  padding: const EdgeInsets.only(top: 10),
                  child: Text(
                    widget.appState.lastError!,
                    style:
                        TextStyle(color: Theme.of(context).colorScheme.error),
                  ),
                ),
              _section(context, context.l10n.settingsAccount),
              _tile(
                context,
                icon: Icons.badge_outlined,
                title: context.l10n.profileEditTitle,
                subtitle: context.l10n.profileEditHint,
                onTap: () => _push(context,
                    ProfileEditScreen(appState: widget.appState)),
              ),
              _tile(
                context,
                icon: Icons.alternate_email_rounded,
                title: context.l10n.settingsAccount,
                subtitle: '${cfg.username}@${cfg.domain}',
                onTap: () => _push(context,
                    EditConfigScreen(appState: widget.appState)),
              ),
              _tile(
                context,
                icon: Icons.person_add_alt_1_outlined,
                title: context.l10n.settingsFollowRequests,
                subtitle: context.l10n.settingsFollowRequestsHint,
                onTap: () => _push(context,
                    FollowRequestsScreen(appState: widget.appState)),
              ),
              _section(context, context.l10n.privacyTitle),
              _tile(
                context,
                icon: Icons.lock_outline_rounded,
                title: context.l10n.privacyTitle,
                subtitle: context.l10n.privacyHint,
                onTap: () => _push(context,
                    PrivacySettingsScreen(appState: widget.appState)),
              ),
              _tile(
                context,
                icon: Icons.shield_outlined,
                title: context.l10n.securityTitle,
                subtitle: context.l10n.securityHint,
                onTap: () => _push(context,
                    SecuritySettingsScreen(appState: widget.appState)),
              ),
              _tile(
                context,
                icon: Icons.gavel_rounded,
                title: context.l10n.moderationTitle,
                subtitle: context.l10n.moderationHintTitle,
                onTap: () => _push(context,
                    ModerationSettingsScreen(appState: widget.appState)),
              ),
              _section(context, context.l10n.networkingTitle),
              _tile(
                context,
                icon: Icons.lan_outlined,
                title: context.l10n.networkingTitle,
                subtitle: context.l10n.networkingHintTitle,
                onTap: () => _push(context,
                    NetworkingSettingsScreen(appState: widget.appState)),
              ),
              _section(context, context.l10n.uiSettingsTitle),
              _tile(
                context,
                icon: Icons.palette_outlined,
                title: context.l10n.uiSettingsTitle,
                subtitle: context.l10n.uiSettingsHint,
                onTap: () => _push(context,
                    UiSettingsScreen(appState: widget.appState)),
              ),
              _tile(
                context,
                icon: Icons.translate_rounded,
                title: context.l10n.translationTitle,
                subtitle: context.l10n.translationHint,
                onTap: () => _push(context,
                    TranslationSettingsScreen(appState: widget.appState)),
              ),
              _section(context, context.l10n.backupTitle),
              _tile(
                context,
                icon: Icons.archive_outlined,
                title: context.l10n.backupTitle,
                subtitle: context.l10n.backupHint,
                onTap: () =>
                    _push(context, BackupScreen(appState: widget.appState)),
              ),
              _tile(
                context,
                icon: Icons.terminal_rounded,
                title: context.l10n.settingsAdvancedDev,
                subtitle: context.l10n.settingsAdvancedDevHint,
                onTap: () =>
                    _push(context, DevCoreScreen(appState: widget.appState)),
              ),
              const SizedBox(height: 20),
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

  Widget _accountHeader(BuildContext context, CoreConfig cfg) {
    final scheme = Theme.of(context).colorScheme;
    final running = widget.appState.isRunning;
    return Container(
      padding: const EdgeInsets.all(UiTokens.padCard),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(UiTokens.radiusCard),
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            scheme.primary.withAlpha(46),
            scheme.secondary.withAlpha(28),
          ],
        ),
        border: Border.all(color: scheme.primary.withAlpha(70)),
      ),
      child: Row(
        children: [
          CircleAvatar(
            radius: 26,
            backgroundColor: scheme.primary.withAlpha(40),
            child: Text(
              cfg.username.isEmpty
                  ? '?'
                  : cfg.username.characters.first.toUpperCase(),
              style: Theme.of(context).textTheme.titleLarge?.copyWith(
                    color: scheme.onSurface,
                    fontWeight: FontWeight.w800,
                  ),
            ),
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${cfg.username}@${cfg.domain}',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 4),
                Row(
                  children: [
                    StatusDot(
                      color: running
                          ? const Color(0xFF3DDC97)
                          : const Color(0xFFF2A33A),
                      size: 7,
                    ),
                    const SizedBox(width: 6),
                    Text(
                      running
                          ? context.l10n.coreOnline
                          : context.l10n.coreOffline,
                      style: Theme.of(context).textTheme.labelSmall,
                    ),
                  ],
                ),
              ],
            ),
          ),
          FilledButton.tonal(
            onPressed: () async {
              if (widget.appState.isRunning) {
                await widget.appState.stopCore();
              } else {
                await widget.appState.startCore();
              }
            },
            child: Text(widget.appState.isRunning
                ? context.l10n.coreStop
                : context.l10n.coreStart),
          ),
        ],
      ),
    );
  }

  Widget _section(BuildContext context, String title) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 20, 4, 8),
      child: Text(
        title.toUpperCase(),
        style: Theme.of(context).textTheme.labelSmall?.copyWith(
              letterSpacing: 1.1,
              fontWeight: FontWeight.w800,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
      ),
    );
  }

  Widget _tile(
    BuildContext context, {
    required IconData icon,
    required String title,
    required String subtitle,
    required VoidCallback onTap,
  }) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Card(
        child: ListTile(
          leading: Container(
            width: 40,
            height: 40,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(12),
              color: scheme.primary.withAlpha(20),
              border: Border.all(color: scheme.primary.withAlpha(60)),
            ),
            child: Icon(icon, size: 20, color: scheme.primary),
          ),
          title: Text(title, style: Theme.of(context).textTheme.titleSmall),
          subtitle: Text(subtitle),
          trailing: const Icon(Icons.chevron_right_rounded),
          onTap: onTap,
        ),
      ),
    );
  }

  void _push(BuildContext context, Widget page) {
    Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => page));
  }
}
