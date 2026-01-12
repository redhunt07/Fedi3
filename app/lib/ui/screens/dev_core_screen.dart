/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

import '../../core/fedi3_core.dart';
import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';

class DevCoreScreen extends StatefulWidget {
  const DevCoreScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<DevCoreScreen> createState() => _DevCoreScreenState();
}

class _DevCoreScreenState extends State<DevCoreScreen> {
  String _status = '';
  String _migrationJson = '';
  bool _statusReady = false;

  @override
  void initState() {
    super.initState();
    _status = '';
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_statusReady) return;
    _status = _safeCoreVersion();
    _statusReady = true;
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config!;
    final running = widget.appState.isRunning;
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.devCoreTitle)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(_status),
          const SizedBox(height: 12),
          Card(
            child: ListTile(
              title: Text(context.l10n.devCoreConfigSaved),
              subtitle: Text(const JsonEncoder.withIndent('  ').convert(cfg.toCoreStartJson())),
            ),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              FilledButton(
                onPressed: running ? null : () async => widget.appState.startCore(),
                child: Text(context.l10n.devCoreStart),
              ),
              OutlinedButton(
                onPressed: running ? () async => widget.appState.stopCore() : null,
                child: Text(context.l10n.devCoreStop),
              ),
              OutlinedButton(
                onPressed: running ? _fetchMigrationStatus : null,
                child: Text(context.l10n.devCoreFetchMigration),
              ),
            ],
          ),
          if (_migrationJson.isNotEmpty) ...[
            const SizedBox(height: 12),
            Card(child: Padding(padding: const EdgeInsets.all(12), child: Text(_migrationJson))),
          ],
        ],
      ),
    );
  }

  String _safeCoreVersion() {
    try {
      return context.l10n.devCoreVersion(Fedi3Core.instance.version());
    } catch (e) {
      return context.l10n.devCoreNotLoaded(e.toString());
    }
  }

  Future<void> _fetchMigrationStatus() async {
    try {
      final cfg = widget.appState.config!;
      final base = cfg.localBaseUri;
      final uri = base.replace(path: '/_fedi3/migration/status');
      final r = await _getJson(uri, cfg.internalToken);
      setState(() => _migrationJson = const JsonEncoder.withIndent('  ').convert(r));
    } catch (e) {
      setState(() => _migrationJson = context.l10n.settingsErr(e.toString()));
    }
  }

  Future<Map<String, dynamic>> _getJson(Uri uri, String token) async {
    final client = HttpClient();
    final req = await client.getUrl(uri);
    if (token.trim().isNotEmpty) {
      req.headers.set('X-Fedi3-Internal', token.trim());
    }
    final resp = await req.close();
    final body = await resp.transform(const Utf8Decoder()).join();
    client.close();
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('HTTP ${resp.statusCode}: $body');
    }
    return jsonDecode(body) as Map<String, dynamic>;
  }
}
