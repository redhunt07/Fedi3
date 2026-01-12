/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'dart:convert';

import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../state/app_state.dart';

class EditConfigScreen extends StatefulWidget {
  const EditConfigScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<EditConfigScreen> createState() => _EditConfigScreenState();
}

class _EditConfigScreenState extends State<EditConfigScreen> {
  late final TextEditingController _username;
  late final TextEditingController _domain;
  late final TextEditingController _publicBaseUrl;
  late final TextEditingController _relayWs;
  late final TextEditingController _relayToken;
  late final TextEditingController _bind;
  late final TextEditingController _internalToken;
  late final TextEditingController _p2pRelayReserve;
  late final TextEditingController _webrtcIceUrls;
  late final TextEditingController _webrtcIceUsername;
  late final TextEditingController _webrtcIceCredential;
  bool _p2pEnable = false;
  bool _webrtcEnable = false;
  String _postDeliveryMode = CoreConfig.postDeliveryP2pRelay;
  bool _customRelay = false;
  bool _discovering = false;
  final List<_RelayOption> _relayOptions = [];
  _RelayOption? _selectedRelay;

  static const List<String> _bootstrapRelays = [
    'https://relay.fedi3.com',
  ];

  @override
  void initState() {
    super.initState();
    final cfg = widget.appState.config!;
    _username = TextEditingController(text: cfg.username);
    _domain = TextEditingController(text: cfg.domain);
    _publicBaseUrl = TextEditingController(text: cfg.publicBaseUrl);
    _relayWs = TextEditingController(text: cfg.relayWs);
    _relayToken = TextEditingController(text: cfg.relayToken);
    _bind = TextEditingController(text: cfg.bind);
    _internalToken = TextEditingController(text: cfg.internalToken);
    _p2pRelayReserve = TextEditingController(text: cfg.p2pRelayReserve.join('\n'));
    _p2pEnable = cfg.p2pEnable;
    _webrtcIceUrls = TextEditingController(text: cfg.webrtcIceUrls.join('\n'));
    _webrtcIceUsername = TextEditingController(text: cfg.webrtcIceUsername ?? '');
    _webrtcIceCredential = TextEditingController(text: cfg.webrtcIceCredential ?? '');
    _webrtcEnable = cfg.webrtcEnable;
    _postDeliveryMode = cfg.postDeliveryMode;
    _seedRelayOptions(cfg.publicBaseUrl);
    _autofillNetworkingDefaults(force: false);
  }

  @override
  void dispose() {
    _username.dispose();
    _domain.dispose();
    _publicBaseUrl.dispose();
    _relayWs.dispose();
    _relayToken.dispose();
    _bind.dispose();
    _internalToken.dispose();
    _p2pRelayReserve.dispose();
    _webrtcIceUrls.dispose();
    _webrtcIceUsername.dispose();
    _webrtcIceCredential.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final running = widget.appState.isRunning;
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.editAccountTitle),
        actions: [
          TextButton(
            onPressed: () => _save(context),
            child: Text(context.l10n.editAccountSave),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          if (running)
            Card(
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Text(
                  context.l10n.editAccountCoreRunningWarning,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            ),
          _field(context.l10n.onboardingUsername, _username),
          _relayPicker(context),
          SwitchListTile(
            title: Text(context.l10n.onboardingRelayCustom),
            value: _customRelay,
            onChanged: (v) => setState(() => _customRelay = v),
          ),
          if (_customRelay) ...[
            _field(context.l10n.onboardingDomain, _domain),
            _field(context.l10n.editAccountRelayPublicUrl, _publicBaseUrl),
            _field(context.l10n.editAccountRelayWs, _relayWs),
          ],
          _field(context.l10n.onboardingRelayToken, _relayToken),
          _field(context.l10n.onboardingBind, _bind),
          _field(context.l10n.onboardingInternalToken, _internalToken),
          _field('P2P relay reserve (multiaddr, una per riga)', _p2pRelayReserve, maxLines: 3),
          SwitchListTile(
            title: Text(context.l10n.onboardingEnableP2p),
            value: _p2pEnable,
            onChanged: (v) => setState(() => _p2pEnable = v),
          ),
          DropdownButtonFormField<String>(
            value: _postDeliveryMode,
            decoration: const InputDecoration(
              labelText: 'Modalità consegna post',
            ),
            items: const [
              DropdownMenuItem(
                value: CoreConfig.postDeliveryP2pRelay,
                child: Text('P2P + Relay (fallback dopo 5s)'),
              ),
              DropdownMenuItem(
                value: CoreConfig.postDeliveryP2pOnly,
                child: Text('Solo P2P (mai relay)'),
              ),
            ],
            onChanged: (value) {
              if (value == null) return;
              setState(() => _postDeliveryMode = value);
            },
          ),
          const SizedBox(height: 6),
          SwitchListTile(
            title: const Text('WebRTC (ICE/TURN)'),
            subtitle: const Text('Usa TURN/STUN come fallback P2P (consigliato)'),
            value: _webrtcEnable,
            onChanged: (v) => setState(() => _webrtcEnable = v),
          ),
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              onPressed: () => setState(() => _autofillNetworkingDefaults(force: true)),
              icon: const Icon(Icons.bolt),
              label: const Text('Auto-popola ICE da relay'),
            ),
          ),
          _field('WebRTC ICE URLs (una per riga)', _webrtcIceUrls, maxLines: 3),
          _field('WebRTC ICE username', _webrtcIceUsername),
          _field('WebRTC ICE credential', _webrtcIceCredential),
          const SizedBox(height: 12),
          OutlinedButton(
            onPressed: () => setState(() => _internalToken.text = CoreConfig.randomToken()),
            child: Text(context.l10n.editAccountRegenerateInternal),
          ),
        ],
      ),
    );
  }

  Widget _field(String label, TextEditingController controller, {int maxLines = 1}) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: TextField(
        controller: controller,
        maxLines: maxLines,
        decoration: InputDecoration(labelText: label),
      ),
    );
  }

  Widget _relayPicker(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(context.l10n.onboardingRelaySelect, style: Theme.of(context).textTheme.labelLarge),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(
                child: DropdownButtonFormField<_RelayOption>(
                  initialValue: _selectedRelay,
                  items: _relayOptions
                      .map((r) => DropdownMenuItem(
                            value: r,
                            child: Text(r.label, overflow: TextOverflow.ellipsis),
                          ))
                      .toList(),
                  onChanged: _discovering
                      ? null
                      : (value) {
                          setState(() => _selectedRelay = value);
                          if (value != null) {
                            _applyRelayOption(value);
                          }
                        },
                  decoration: InputDecoration(
                    hintText: context.l10n.onboardingRelayPick,
                  ),
                ),
              ),
              const SizedBox(width: 10),
              FilledButton.icon(
                onPressed: _discovering ? null : _discoverRelays,
                icon: _discovering ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)) : const Icon(Icons.public),
                label: Text(_discovering ? context.l10n.onboardingRelayDiscovering : context.l10n.onboardingRelayDiscover),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            context.l10n.onboardingRelayListHint,
            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160), fontSize: 12),
          ),
        ],
      ),
    );
  }

  void _seedRelayOptions(String current) {
    final set = <String>{};
    final currentOpt = _relayOptionFromUrl(current);
    if (currentOpt != null) {
      _relayOptions.add(currentOpt);
      set.add(currentOpt.publicUrl);
      _selectedRelay = currentOpt;
      _applyRelayOption(currentOpt);
    }
    for (final url in _bootstrapRelays) {
      final opt = _relayOptionFromUrl(url);
      if (opt != null && set.add(opt.publicUrl)) {
        _relayOptions.add(opt);
      }
    }
  }

  Future<void> _discoverRelays() async {
    setState(() => _discovering = true);
    try {
      final found = <_RelayOption>[];
      final seen = <String>{};
      for (final base in _bootstrapRelays) {
        final uri = Uri.parse(base).replace(path: '/_fedi3/relay/relays');
        final resp = await http.get(uri);
        if (resp.statusCode < 200 || resp.statusCode >= 300) continue;
        final json = jsonDecode(resp.body);
        if (json is! Map) continue;
        final relays = json['relays'];
        if (relays is! List) continue;
        for (final item in relays) {
          if (item is! Map) continue;
          final url = (item['relay_url'] as String?)?.trim();
          if (url == null || url.isEmpty) continue;
          final opt = _relayOptionFromUrl(url);
          if (opt != null && seen.add(opt.publicUrl)) {
            found.add(opt);
          }
        }
      }
      if (found.isNotEmpty) {
        setState(() {
          _relayOptions
            ..clear()
            ..addAll(found);
          _selectedRelay = _relayOptions.first;
          _applyRelayOption(_selectedRelay!);
        });
      }
    } catch (_) {
      // Silent discovery failure; user can still enter values manually.
    } finally {
      if (mounted) setState(() => _discovering = false);
    }
  }

  void _applyRelayOption(_RelayOption relay) {
    _publicBaseUrl.text = relay.publicUrl;
    _relayWs.text = relay.wsUrl;
    _domain.text = relay.domain;
    _customRelay = false;
    _autofillNetworkingDefaults(force: true);
  }

  _RelayOption? _relayOptionFromUrl(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return null;
    final scheme = uri.scheme.isEmpty ? 'https' : uri.scheme;
    final publicUrl = uri.replace(scheme: scheme).toString().trim().replaceAll(RegExp(r'/$'), '');
    final wsScheme = scheme == 'https' ? 'wss' : 'ws';
    final wsUrl = uri.replace(scheme: wsScheme).toString().trim().replaceAll(RegExp(r'/$'), '');
    final domain = uri.host;
    return _RelayOption(
      publicUrl: publicUrl,
      wsUrl: wsUrl,
      domain: domain,
      label: uri.host,
    );
  }

  void _autofillNetworkingDefaults({required bool force}) {
    final domain = _domain.text.trim().isNotEmpty
        ? _domain.text.trim()
        : (Uri.tryParse(_publicBaseUrl.text.trim())?.host ?? '');
    if (domain.isEmpty) return;
    if (_webrtcEnable == false && _p2pEnable) {
      _webrtcEnable = true;
    }
    if (force || _webrtcIceUrls.text.trim().isEmpty) {
      _webrtcIceUrls.text = [
        'stun:$domain:3478',
        'turn:$domain:3478?transport=udp',
        'turn:$domain:3478?transport=tcp',
      ].join('\n');
    }
    _autofillP2pRelayReserve(force: force, domain: domain);
  }

  Future<void> _autofillP2pRelayReserve({required bool force, required String domain}) async {
    if (!force && _p2pRelayReserve.text.trim().isNotEmpty) {
      return;
    }
    final base = _publicBaseUrl.text.trim().isNotEmpty ? _publicBaseUrl.text.trim() : 'https://$domain';
    final uri = Uri.tryParse('$base/_fedi3/relay/p2p_infra');
    if (uri == null) return;
    try {
      final resp = await http.get(uri).timeout(const Duration(seconds: 5));
      if (resp.statusCode != 200) return;
      final data = jsonDecode(resp.body);
      if (data is! Map) return;
      final items = data['multiaddrs'];
      if (items is! List) return;
      final multiaddrs = items
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList();
      if (multiaddrs.isEmpty) return;
      _p2pRelayReserve.text = multiaddrs.join('\n');
    } catch (_) {
      // Silent fallback; user can still enter multiaddrs manually.
    }
  }

  Future<void> _save(BuildContext context) async {
    final relayToken = _relayToken.text.trim();
    if (relayToken.length < 16) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.onboardingRelayTokenTooShort)),
      );
      return;
    }
    final cfg = CoreConfig(
      username: _username.text.trim(),
      domain: _domain.text.trim(),
      publicBaseUrl: _publicBaseUrl.text.trim(),
      relayWs: _relayWs.text.trim(),
      relayToken: relayToken,
      bind: _bind.text.trim(),
      internalToken: _internalToken.text.trim(),
      p2pEnable: _p2pEnable,
      p2pRelayReserve: _p2pRelayReserve.text
          .split('\n')
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      webrtcEnable: _webrtcEnable,
      webrtcIceUrls: _webrtcIceUrls.text
          .split('\n')
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      webrtcIceUsername: _webrtcIceUsername.text.trim().isEmpty ? null : _webrtcIceUsername.text.trim(),
      webrtcIceCredential: _webrtcIceCredential.text.trim().isEmpty ? null : _webrtcIceCredential.text.trim(),
      apRelays: widget.appState.config?.apRelays ?? const [],
      displayName: widget.appState.config?.displayName ?? '',
      summary: widget.appState.config?.summary ?? '',
      iconUrl: widget.appState.config?.iconUrl ?? '',
      iconMediaType: widget.appState.config?.iconMediaType ?? '',
      imageUrl: widget.appState.config?.imageUrl ?? '',
      imageMediaType: widget.appState.config?.imageMediaType ?? '',
      profileFields: widget.appState.config?.profileFields ?? const [],
      manuallyApprovesFollowers: widget.appState.config?.manuallyApprovesFollowers ?? false,
      blockedDomains: widget.appState.config?.blockedDomains ?? const [],
      blockedActors: widget.appState.config?.blockedActors ?? const [],
      postDeliveryMode: _postDeliveryMode,
      p2pCacheTtlSecs: widget.appState.config?.p2pCacheTtlSecs,
      previousPublicBaseUrl: widget.appState.config?.previousPublicBaseUrl,
      previousRelayToken: widget.appState.config?.previousRelayToken,
    );

    await widget.appState.stopCore();
    await widget.appState.saveConfig(cfg);
    if (!context.mounted) return;
    Navigator.of(context).pop();
  }
}

class _RelayOption {
  _RelayOption({
    required this.publicUrl,
    required this.wsUrl,
    required this.domain,
    required this.label,
  });

  final String publicUrl;
  final String wsUrl;
  final String domain;
  final String label;
}
