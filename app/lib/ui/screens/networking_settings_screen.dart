/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../model/relay_directory.dart';
import '../../services/relay_directory_service.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/network_error_card.dart';
import '../widgets/relay_latency_badge.dart';
import 'relay_admin_screen.dart';
import 'edit_config_screen.dart';

class NetworkingSettingsScreen extends StatefulWidget {
  const NetworkingSettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<NetworkingSettingsScreen> createState() =>
      _NetworkingSettingsScreenState();
}

class _NetworkingSettingsScreenState extends State<NetworkingSettingsScreen> {
  bool _loadingRelays = false;
  String? _relayError;
  List<Map<String, dynamic>> _relays = const [];

  bool _loadingDirectory = false;
  String? _directoryError;
  List<RelayLatency> _directoryLatencies = const [];
  DateTime? _directoryMeasuredAt;

  late final TextEditingController _anchorActorUrl;
  String? _anchorError;
  String? _anchorStatus;
  bool _savingAnchor = false;
  late final TextEditingController _bootstrap;
  late bool _publicDiscovery;
  late bool _peerExchange;
  bool _savingDiscovery = false;
  String? _discoveryStatus;

  @override
  void initState() {
    super.initState();
    _anchorActorUrl = TextEditingController(
        text: widget.appState.config?.anchorActorUrl ?? '');
    final cfg = widget.appState.config;
    _bootstrap =
        TextEditingController(text: cfg?.p2pBootstrap.join('\n') ?? '');
    _publicDiscovery = cfg?.p2pPublicDiscovery ?? true;
    _peerExchange = cfg?.p2pPeerExchange ?? true;
    _loadRelays();
    _loadDirectory();
  }

  @override
  void dispose() {
    _anchorActorUrl.dispose();
    _bootstrap.dispose();
    super.dispose();
  }

  Future<void> _saveAnchor() async {
    final raw = _anchorActorUrl.text.trim();
    final normalized = CoreConfig.normalizeAnchorActorUrl(raw);
    if (raw.isNotEmpty && normalized == null) {
      setState(() {
        _anchorError = context.l10n.networkingAnchorInvalid;
        _anchorStatus = null;
      });
      return;
    }
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() {
      _anchorError = null;
      _anchorStatus = null;
      _savingAnchor = true;
    });
    try {
      // Same update path used for the other networking/account fields: stop,
      // persist and restart so the anchor takes effect immediately.
      await widget.appState
          .replaceConfig(cfg.copyWith(anchorActorUrl: normalized));
      if (!mounted) return;
      _anchorActorUrl.text = normalized ?? '';
      setState(() => _anchorStatus = context.l10n.ok);
    } catch (e) {
      if (!mounted) return;
      setState(() => _anchorStatus = context.l10n.err(e.toString()));
    } finally {
      if (mounted) setState(() => _savingAnchor = false);
    }
  }

  Future<void> _saveDiscovery() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() {
      _savingDiscovery = true;
      _discoveryStatus = null;
    });
    try {
      await widget.appState.replaceConfig(cfg.copyWith(
        p2pBootstrap: _bootstrap.text
            .split('\n')
            .map((s) => s.trim())
            .where((s) => s.isNotEmpty)
            .toSet()
            .toList(),
        p2pPublicDiscovery: _publicDiscovery,
        p2pPeerExchange: _peerExchange,
      ));
      if (mounted) setState(() => _discoveryStatus = context.l10n.ok);
    } catch (e) {
      if (mounted) {
        setState(() => _discoveryStatus = context.l10n.err(e.toString()));
      }
    } finally {
      if (mounted) setState(() => _savingDiscovery = false);
    }
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

  Future<void> _loadDirectory() async {
    setState(() {
      _loadingDirectory = true;
      _directoryError = null;
    });
    try {
      RelayDirectorySnapshot? snapshot;
      Object? fetchError;
      try {
        snapshot = await RelayDirectoryService.fetch();
      } catch (e) {
        fetchError = e;
        snapshot = await RelayDirectoryService.cached() ??
            await RelayDirectoryService.bundled();
      }
      if (snapshot == null) {
        throw fetchError ?? StateError('relay directory unavailable');
      }
      final latencies = await RelayDirectoryService.probeAll(snapshot.entries);
      if (!mounted) return;
      setState(() {
        _directoryLatencies = latencies;
        _directoryMeasuredAt = DateTime.now();
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _directoryError = e.toString());
    } finally {
      if (mounted) setState(() => _loadingDirectory = false);
    }
  }

  Widget _directorySection(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final muted = scheme.onSurface.withAlpha(160);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _section(context, context.l10n.networkingRelayDirectoryTitle),
        Text(context.l10n.networkingRelayDirectoryHint,
            style: TextStyle(color: muted, fontSize: 12)),
        const SizedBox(height: 8),
        Row(
          children: [
            Text(
              _directoryLatencies.isEmpty
                  ? context.l10n.relayDirectoryEmpty
                  : context.l10n
                      .networkingRelaysCount(_directoryLatencies.length),
            ),
            const Spacer(),
            IconButton(
              tooltip: context.l10n.relayDirectoryRefresh,
              onPressed: _loadingDirectory ? null : _loadDirectory,
              icon: _loadingDirectory
                  ? const Icon(Icons.hourglass_top)
                  : const Icon(Icons.speed),
            ),
          ],
        ),
        if (_directoryError != null)
          NetworkErrorCard(
            message: context.l10n.relayDirectoryError(_directoryError!),
            onRetry: _loadDirectory,
            compact: true,
          ),
        if (_directoryLatencies.isNotEmpty)
          GlassPanel(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Column(
              children: [
                for (var i = 0; i < _directoryLatencies.length; i++)
                  ListTile(
                    dense: true,
                    leading: Text('#${i + 1}',
                        style: TextStyle(
                            color: scheme.onSurfaceVariant,
                            fontSize: 11,
                            fontWeight: FontWeight.w700)),
                    title: Text(_directoryLatencies[i].entry.name,
                        maxLines: 1, overflow: TextOverflow.ellipsis),
                    subtitle: Text(_directoryLatencies[i].entry.host,
                        maxLines: 1, overflow: TextOverflow.ellipsis),
                    trailing:
                        RelayLatencyBadge(latency: _directoryLatencies[i]),
                  ),
              ],
            ),
          ),
        if (_directoryMeasuredAt != null)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(
              context.l10n
                  .relayLatencyMeasuredAt(_formatClock(_directoryMeasuredAt!)),
              style: TextStyle(color: muted, fontSize: 11),
            ),
          ),
        const SizedBox(height: 8),
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton.icon(
            onPressed: () => launchUrl(Uri.parse(RelayDirectory.submitUrl),
                mode: LaunchMode.externalApplication),
            icon: const Icon(Icons.add, size: 16),
            label: Text(context.l10n.relayDirectoryAddYours),
          ),
        ),
      ],
    );
  }

  String _formatClock(DateTime value) {
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(value.hour)}:${two(value.minute)}:${two(value.second)}';
  }

  Widget _section(BuildContext context, String title) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 18, 4, 8),
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

  Widget _infoTile(
    BuildContext context, {
    required IconData icon,
    required String title,
    required String value,
  }) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: GlassPanel(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        child: Row(
          children: [
            Container(
              width: 34,
              height: 34,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(11),
                gradient: LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: [
                    scheme.primary.withAlpha(220),
                    const Color(UiTokens.brandCyan).withAlpha(190),
                  ],
                ),
              ),
              child: Icon(icon, size: 18, color: Colors.white),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title,
                      style: const TextStyle(
                          fontWeight: FontWeight.w800, fontSize: 13)),
                  const SizedBox(height: 2),
                  Text(value,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          fontSize: 12, color: scheme.onSurfaceVariant)),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config!;
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.networkingTitle)),
      body: ListView(
        padding: const EdgeInsets.all(UiTokens.padScreen),
        children: [
          _section(context, context.l10n.networkingRelay),
          _infoTile(
            context,
            icon: Icons.public,
            title: context.l10n.networkingRelay,
            value: cfg.publicBaseUrl,
          ),
          _infoTile(
            context,
            icon: Icons.cable_rounded,
            title: context.l10n.networkingRelayWs,
            value: cfg.relayWs,
          ),
          _infoTile(
            context,
            icon: Icons.settings_ethernet_rounded,
            title: context.l10n.networkingBind,
            value: cfg.bind,
          ),
          _infoTile(
            context,
            icon: Icons.hub_rounded,
            title: context.l10n.networkingP2p,
            value: cfg.p2pEnable ? context.l10n.enabled : context.l10n.disabled,
          ),
          _section(context, context.l10n.networkingDiscoveryTitle),
          GlassPanel(
            padding: const EdgeInsets.all(16),
            child:
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Material(
                color: Colors.transparent,
                child: SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: Text(context.l10n.networkingPublicDiscovery),
                  subtitle: Text(context.l10n.networkingPublicDiscoveryHint),
                  value: _publicDiscovery,
                  onChanged: _savingDiscovery
                      ? null
                      : (v) => setState(() => _publicDiscovery = v),
                ),
              ),
              Material(
                color: Colors.transparent,
                child: SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: Text(context.l10n.networkingPeerExchange),
                  subtitle: Text(context.l10n.networkingPeerExchangeHint),
                  value: _peerExchange,
                  onChanged: _savingDiscovery
                      ? null
                      : (v) => setState(() => _peerExchange = v),
                ),
              ),
              TextField(
                key: const Key('p2pBootstrapField'),
                controller: _bootstrap,
                enabled: !_savingDiscovery,
                minLines: 2,
                maxLines: 5,
                decoration: InputDecoration(
                    labelText: context.l10n.networkingBootstrapLabel,
                    helperText: context.l10n.networkingBootstrapHint,
                    helperMaxLines: 3),
              ),
              const SizedBox(height: 12),
              FilledButton(
                  key: const Key('saveDiscoveryButton'),
                  onPressed: _savingDiscovery ? null : _saveDiscovery,
                  child: Text(context.l10n.save)),
              if (_discoveryStatus != null) Text(_discoveryStatus!),
            ]),
          ),
          _directorySection(context),
          _section(context, context.l10n.networkingRelaysTitle),
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
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.refresh),
              ),
            ],
          ),
          if (_relays.isEmpty && !_loadingRelays)
            Text(context.l10n.networkingRelaysEmpty),
          GlassPanel(
            padding: EdgeInsets.zero,
            child: Material(
              color: Colors.transparent,
              child: ListTile(
                title: Text(context.l10n.relayAdminTitle),
                subtitle: Text(context.l10n.relayAdminHint),
                trailing: const Icon(Icons.chevron_right),
                onTap: () {
                  Navigator.of(context).push(
                    MaterialPageRoute(
                        builder: (_) =>
                            RelayAdminScreen(appState: widget.appState)),
                  );
                },
              ),
            ),
          ),
          if (_relays.isNotEmpty)
            for (final r in _relays)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: GlassPanel(
                  padding: EdgeInsets.zero,
                  child: Material(
                    color: Colors.transparent,
                    child: ListTile(
                      title: Text((r['relay_base_url'] ?? r['relay_url'] ?? '')
                          .toString()),
                      subtitle: Text((r['relay_ws_url'] ?? r['relay_ws'] ?? '')
                          .toString()),
                    ),
                  ),
                ),
              ),
          _section(context, context.l10n.networkingApRelays),
          if (cfg.apRelays.isEmpty)
            Text(context.l10n.networkingApRelaysEmpty)
          else
            for (final r in cfg.apRelays)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: GlassPanel(
                  padding: EdgeInsets.zero,
                  child: Material(
                    color: Colors.transparent,
                    child: ListTile(title: Text(r)),
                  ),
                ),
              ),
          _section(context, context.l10n.networkingAnchorTitle),
          TextField(
            key: const Key('anchorActorUrlField'),
            controller: _anchorActorUrl,
            keyboardType: TextInputType.url,
            autocorrect: false,
            decoration: InputDecoration(
              labelText: context.l10n.networkingAnchorLabel,
              hintText: 'https://alice.example',
              helperText: context.l10n.networkingAnchorHint,
              helperMaxLines: 4,
              errorText: _anchorError,
            ),
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              FilledButton.icon(
                key: const Key('saveAnchorButton'),
                onPressed: _savingAnchor ? null : _saveAnchor,
                icon: _savingAnchor
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.link),
                label: Text(context.l10n.networkingAnchorSave),
              ),
              if (_anchorStatus != null)
                Text(
                  _anchorStatus!,
                  style: _anchorStatus == context.l10n.ok
                      ? null
                      : TextStyle(color: Theme.of(context).colorScheme.error),
                ),
            ],
          ),
          const SizedBox(height: 12),
          OutlinedButton.icon(
            onPressed: () {
              Navigator.of(context).push(MaterialPageRoute(
                  builder: (_) => EditConfigScreen(appState: widget.appState)));
            },
            icon: const Icon(Icons.tune),
            label: Text(context.l10n.networkingEditAccount),
          ),
          const SizedBox(height: 16),
          Text(
            context.l10n.networkingHint,
            style: TextStyle(
                color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
          ),
        ],
      ),
    );
  }
}
