/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/core_service_control.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import 'relay_admin_screen.dart';
import 'edit_config_screen.dart';

class NetworkingSettingsScreen extends StatefulWidget {
  const NetworkingSettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<NetworkingSettingsScreen> createState() => _NetworkingSettingsScreenState();
}

class _NetworkingSettingsScreenState extends State<NetworkingSettingsScreen> {
  static const List<String> _bootstrapRelays = [
    'https://relay.fedi3.com',
    'https://relay.foxyhole.io',
  ];
  bool _loadingRelays = false;
  bool _restartingCore = false;
  bool _restartingService = false;
  String? _relayError;
  List<Map<String, dynamic>> _relays = const [];

  @override
  void initState() {
    super.initState();
    _loadRelays();
  }

  Future<void> _loadRelays() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() {
      _loadingRelays = true;
      _relayError = null;
    });
    try {
      final api = CoreApi(config: cfg);
      final resp = await api.fetchRelays();
      final items = <Map<String, dynamic>>[];
      final raw = (resp['items'] is List) ? resp['items'] : resp['relays'];
      if (raw is List) {
        for (final it in raw) {
          if (it is Map) items.add(it.cast<String, dynamic>());
        }
      }
      final merged = _mergeRelayFallback(items, cfg.publicBaseUrl);
      if (mounted) setState(() => _relays = merged);
    } catch (e) {
      final fallback = _mergeRelayFallback(const [], cfg.publicBaseUrl);
      if (mounted) {
        setState(() {
          _relayError = e.toString();
          _relays = fallback;
        });
      }
    } finally {
      if (mounted) setState(() => _loadingRelays = false);
    }
  }

  List<Map<String, dynamic>> _mergeRelayFallback(
    List<Map<String, dynamic>> relays,
    String currentRelay,
  ) {
    final merged = <Map<String, dynamic>>[];
    final seen = <String>{};
    for (final relay in relays) {
      final url = (relay['relay_base_url'] ?? relay['relay_url'] ?? relay['base'])
          ?.toString()
          .trim();
      if (url == null || url.isEmpty) continue;
      if (seen.add(url)) {
        merged.add(relay);
      }
    }
    if (merged.length > 1) {
      return merged;
    }
    final base = currentRelay.trim();
    if (base.isNotEmpty && seen.add(base)) {
      merged.add({'relay_base_url': base, 'relay_ws_url': _defaultWs(base)});
    }
    for (final relay in _bootstrapRelays) {
      if (seen.add(relay)) {
        merged.add({'relay_base_url': relay, 'relay_ws_url': _defaultWs(relay)});
      }
    }
    return merged;
  }

  String _defaultWs(String relayBase) {
    if (relayBase.startsWith('https://')) {
      return relayBase.replaceFirst('https://', 'wss://');
    }
    if (relayBase.startsWith('http://')) {
      return relayBase.replaceFirst('http://', 'ws://');
    }
    return relayBase;
  }

  Future<void> _refreshRelays() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      await CoreApi(config: cfg).refreshRelays();
      await _loadRelays();
    } catch (e) {
      if (mounted) setState(() => _relayError = e.toString());
    }
  }

  Future<void> _restartCoreNow() async {
    if (_restartingCore) return;
    setState(() => _restartingCore = true);
    try {
      await widget.appState.stopCore();
      await widget.appState.startCore();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.networkingRestartCoreRequested)),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.settingsErr(e.toString()))),
      );
    } finally {
      if (mounted) setState(() => _restartingCore = false);
    }
  }

  Future<void> _restartBackgroundService() async {
    if (_restartingService) return;
    setState(() => _restartingService = true);
    try {
      final res = await CoreServiceControl.restartBackgroundService();
      if (!mounted) return;
      if (res.ok) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(res.message)),
        );
        await widget.appState.startCore();
        return;
      }
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(context.l10n.networkingRestartServiceFailedTitle),
          content: SelectableText(
            res.manualCommand.trim().isEmpty
                ? res.message
                : '${res.message}\n\nManual command:\n${res.manualCommand}',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: Text(context.l10n.updateDismiss),
            ),
          ],
        ),
      );
    } finally {
      if (mounted) setState(() => _restartingService = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config!;
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.networkingTitle)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Card(
            child: ListTile(
              title: Text(context.l10n.networkingRelay),
              subtitle: Text(cfg.publicBaseUrl),
            ),
          ),
          const SizedBox(height: 10),
          Card(
            child: ListTile(
              title: Text(context.l10n.networkingRelayWs),
              subtitle: Text(cfg.relayWs),
            ),
          ),
          const SizedBox(height: 10),
          Card(
            child: ListTile(
              title: Text(context.l10n.networkingBind),
              subtitle: Text(cfg.bind),
            ),
          ),
          const SizedBox(height: 10),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    context.l10n.networkingCoreControlTitle,
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                  const SizedBox(height: 10),
                  Wrap(
                    spacing: 10,
                    runSpacing: 10,
                    children: [
                      OutlinedButton.icon(
                        onPressed: _restartingCore ? null : _restartCoreNow,
                        icon: _restartingCore
                            ? const SizedBox(
                                width: 14,
                                height: 14,
                                child: CircularProgressIndicator(strokeWidth: 2),
                              )
                            : const Icon(Icons.restart_alt),
                        label: Text(context.l10n.networkingRestartCoreNow),
                      ),
                      if (widget.appState.isExternalCore)
                        OutlinedButton.icon(
                          onPressed:
                              _restartingService ? null : _restartBackgroundService,
                          icon: _restartingService
                              ? const SizedBox(
                                  width: 14,
                                  height: 14,
                                  child: CircularProgressIndicator(strokeWidth: 2),
                                )
                              : const Icon(Icons.settings_backup_restore),
                          label: Text(
                              context.l10n.networkingRestartBackgroundService),
                        ),
                    ],
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
          Text(context.l10n.networkingRelaysTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          if (_relayError != null)
            NetworkErrorCard(
              message: _relayError ?? context.l10n.networkingRelaysError,
              onRetry: _refreshRelays,
              compact: true,
            ),
          Row(
            children: [
              Text(context.l10n.networkingRelaysCount(_relays.length)),
              const Spacer(),
              IconButton(
                onPressed: _loadingRelays ? null : _refreshRelays,
                icon: _loadingRelays
                    ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.refresh),
              ),
            ],
          ),
          if (_relays.isEmpty && !_loadingRelays)
            Text(context.l10n.networkingRelaysEmpty),
          Card(
            child: ListTile(
              title: Text(context.l10n.relayAdminTitle),
              subtitle: Text(context.l10n.relayAdminHint),
              trailing: const Icon(Icons.chevron_right),
              onTap: () {
                Navigator.of(context).push(
                  MaterialPageRoute(builder: (_) => RelayAdminScreen(appState: widget.appState)),
                );
              },
            ),
          ),
          if (_relays.isNotEmpty)
            for (final r in _relays)
              Card(
                child: ListTile(
                  title: Text((r['relay_base_url'] ?? r['relay_url'] ?? '').toString()),
                  subtitle: Text((r['relay_ws_url'] ?? r['relay_ws'] ?? '').toString()),
                ),
              ),
          const SizedBox(height: 16),
          Text(context.l10n.networkingApRelays, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          if (cfg.apRelays.isEmpty)
            Text(context.l10n.networkingApRelaysEmpty)
          else
            for (final r in cfg.apRelays)
              Card(
                child: ListTile(
                  title: Text(r),
                ),
              ),
          const SizedBox(height: 12),
          OutlinedButton.icon(
            onPressed: () {
              Navigator.of(context).push(MaterialPageRoute(builder: (_) => EditConfigScreen(appState: widget.appState)));
            },
            icon: const Icon(Icons.tune),
            label: Text(context.l10n.networkingEditAccount),
          ),
          const SizedBox(height: 16),
          Text(
            context.l10n.networkingHint,
            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
          ),
        ],
      ),
    );
  }
}
