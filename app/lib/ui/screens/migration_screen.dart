/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';

class MigrationScreen extends StatefulWidget {
  const MigrationScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<MigrationScreen> createState() => _MigrationScreenState();
}

class _MigrationScreenState extends State<MigrationScreen> {
  final TextEditingController _aliasesCtrl = TextEditingController();
  Map<String, dynamic>? _status;
  String? _statusError;
  bool _loadingStatus = false;
  bool _savingAliases = false;

  @override
  void initState() {
    super.initState();
    if (widget.appState.isRunning) {
      _fetchStatus();
    }
  }

  @override
  void dispose() {
    _aliasesCtrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final running = widget.appState.isRunning;
    final status = _status;

    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.migrationTitle)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(context.l10n.migrationHint),
          const SizedBox(height: 12),
          if (!running)
            Card(
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Text(context.l10n.migrationCoreNotRunning),
              ),
            ),
          if (running) ...[
            Card(
              child: ListTile(
                title: Text(context.l10n.migrationStatusTitle),
                subtitle: Text(
                  status == null
                      ? context.l10n.migrationStatusEmpty
                      : context.l10n.migrationStatusReady,
                ),
                trailing: _loadingStatus
                    ? const SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : IconButton(
                        tooltip: context.l10n.migrationRefresh,
                        onPressed: _fetchStatus,
                        icon: const Icon(Icons.refresh),
                      ),
              ),
            ),
            if (_statusError != null) ...[
              const SizedBox(height: 10),
              Card(
                color: Theme.of(context).colorScheme.errorContainer,
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Text(_statusError!),
                ),
              ),
            ],
            if (status != null) ...[
              const SizedBox(height: 10),
              _StatusCard(status: status),
            ],
            const SizedBox(height: 12),
            Card(
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(context.l10n.migrationAliasesTitle,
                        style: Theme.of(context).textTheme.titleMedium),
                    const SizedBox(height: 8),
                    Text(context.l10n.migrationAliasesHint),
                    const SizedBox(height: 10),
                    TextField(
                      controller: _aliasesCtrl,
                      minLines: 3,
                      maxLines: 8,
                      decoration: InputDecoration(
                        border: const OutlineInputBorder(),
                        hintText: context.l10n.migrationAliasesPlaceholder,
                      ),
                    ),
                    const SizedBox(height: 10),
                    Row(
                      children: [
                        FilledButton(
                          onPressed: _savingAliases ? null : _saveAliases,
                          child: _savingAliases
                              ? const SizedBox(
                                  width: 16,
                                  height: 16,
                                  child: CircularProgressIndicator(strokeWidth: 2),
                                )
                              : Text(context.l10n.migrationSaveAliases),
                        ),
                        const SizedBox(width: 10),
                        Text(context.l10n.migrationRestartNote),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _fetchStatus() async {
    if (!widget.appState.isRunning) return;
    setState(() {
      _loadingStatus = true;
      _statusError = null;
    });
    try {
      final cfg = widget.appState.config!;
      final api = CoreApi(config: cfg);
      final status = await api.fetchMigrationStatus();
      _syncAliasesFromStatus(status);
      setState(() => _status = status);
    } catch (e) {
      setState(() => _statusError = context.l10n.settingsErr(e.toString()));
    } finally {
      if (mounted) {
        setState(() => _loadingStatus = false);
      }
    }
  }

  void _syncAliasesFromStatus(Map<String, dynamic> status) {
    final aliases = status['legacy_aliases'];
    if (aliases is List && aliases.isNotEmpty) {
      final lines = aliases.map((e) => e.toString()).join('\n');
      _aliasesCtrl.text = lines;
    }
  }

  Future<void> _saveAliases() async {
    if (!widget.appState.isRunning) return;
    setState(() => _savingAliases = true);
    try {
      final cfg = widget.appState.config!;
      final api = CoreApi(config: cfg);
      final aliases = _parseAliases(_aliasesCtrl.text);
      final result = await api.setLegacyAliases(aliases);
      final msg = result['restart_required'] == true
          ? context.l10n.migrationSavedRestart
          : context.l10n.migrationSaved;
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.settingsErr(e.toString()))),
        );
      }
    } finally {
      if (mounted) {
        setState(() => _savingAliases = false);
      }
    }
  }

  List<String> _parseAliases(String input) {
    return input
        .split(RegExp(r'[\n,]'))
        .map((e) => e.trim())
        .where((e) => e.isNotEmpty)
        .toList();
  }
}

class _StatusCard extends StatelessWidget {
  const _StatusCard({required this.status});

  final Map<String, dynamic> status;

  @override
  Widget build(BuildContext context) {
    final relayMigration = status['relay_migration'] as Map?;
    final legacyGuides = status['legacy_guides'];
    final legacyGuidesText = legacyGuides == null
        ? ''
        : const JsonEncoder.withIndent('  ').convert(legacyGuides);

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _row(context.l10n.migrationActor, status['actor']),
            _row(context.l10n.migrationBaseUrl, status['public_base_url']),
            _row(context.l10n.migrationFollowers, status['followers_count']),
            _row(context.l10n.migrationLegacyFollowers, status['legacy_followers_count']),
            if (relayMigration != null) ...[
              _row(
                context.l10n.migrationHasPreviousAlias,
                relayMigration['has_previous_actor_alias'],
              ),
              _row(context.l10n.migrationNote, relayMigration['note']),
            ],
            if (legacyGuidesText.isNotEmpty) ...[
              const SizedBox(height: 8),
              Text(context.l10n.migrationLegacyGuides,
                  style: Theme.of(context).textTheme.titleSmall),
              const SizedBox(height: 6),
              Text(legacyGuidesText),
            ],
          ],
        ),
      ),
    );
  }

  Widget _row(String label, Object? value) {
    final text = value == null ? '-' : value.toString();
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(width: 160, child: Text(label)),
          Expanded(child: Text(text)),
        ],
      ),
    );
  }
}
