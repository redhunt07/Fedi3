/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
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
  bool _loadingRelays = false;
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
      final raw = resp['items'];
      if (raw is List) {
        for (final it in raw) {
          if (it is Map) items.add(it.cast<String, dynamic>());
        }
      }
      if (mounted) setState(() => _relays = items);
    } catch (e) {
      if (mounted) setState(() => _relayError = e.toString());
    } finally {
      if (mounted) setState(() => _loadingRelays = false);
    }
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
