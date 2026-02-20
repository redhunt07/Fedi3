/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'dart:convert';

import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/backup_codec.dart';
import '../../services/cloud_backup_service.dart';
import '../../state/app_state.dart';

enum OnboardingMode { loginExisting, createNew }

class OnboardingScreen extends StatefulWidget {
  const OnboardingScreen({super.key, required this.appState, this.mode = OnboardingMode.createNew});

  final AppState appState;
  final OnboardingMode mode;

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  late final TextEditingController _username;
  late final TextEditingController _domain;
  late final TextEditingController _publicBaseUrl;
  late final TextEditingController _relayWs;
  late final TextEditingController _relayToken;
  late final TextEditingController _bind;
  late final TextEditingController _internalToken;
  bool _customRelay = false;
  bool _importing = false;
  bool _discovering = false;
  final List<_RelayOption> _relayOptions = [];
  _RelayOption? _selectedRelay;

  static const List<String> _bootstrapRelays = [
    'https://relay.fedi3.com',
  ];

  @override
  void initState() {
    super.initState();
    final isCreate = widget.mode == OnboardingMode.createNew;
    _username = TextEditingController(text: isCreate ? 'alice' : '');
    _domain = TextEditingController(text: isCreate ? 'example.invalid' : '');
    _publicBaseUrl = TextEditingController(text: isCreate ? 'http://127.0.0.1:8787' : '');
    _relayWs = TextEditingController(text: isCreate ? 'ws://127.0.0.1:8787' : '');
    _relayToken = TextEditingController(text: CoreConfig.randomToken());
    _bind = TextEditingController(text: isCreate ? '127.0.0.1:8788' : '127.0.0.1:8788');
    _internalToken = TextEditingController(text: CoreConfig.randomToken());
    _seedRelayOptions();
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
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(
          widget.mode == OnboardingMode.createNew ? context.l10n.onboardingCreateTitle : context.l10n.onboardingLoginTitle,
        ),
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            widget.mode == OnboardingMode.createNew ? context.l10n.onboardingCreateIntro : context.l10n.onboardingLoginIntro,
          ),
          if (widget.mode == OnboardingMode.loginExisting) ...[
            const SizedBox(height: 12),
            Wrap(
              spacing: 10,
              runSpacing: 10,
              children: [
                FilledButton.icon(
                  onPressed: _importing ? null : _importBackupFile,
                  icon: const Icon(Icons.upload_file),
                  label: Text(context.l10n.onboardingImportBackup),
                ),
                OutlinedButton.icon(
                  onPressed: _importing ? null : _importBackupCloud,
                  icon: const Icon(Icons.cloud_download),
                  label: Text(context.l10n.onboardingImportBackupCloud),
                ),
              ],
            ),
          ],
          const SizedBox(height: 16),
          _field(context.l10n.onboardingUsername, _username),
          _relayPicker(context),
          if (_relayOptions.isEmpty || _customRelay) ...[
            _field(context.l10n.onboardingDomain, _domain),
            _field(context.l10n.onboardingRelayPublicUrl, _publicBaseUrl),
            _field(context.l10n.onboardingRelayWs, _relayWs),
          ] else
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () => setState(() => _customRelay = true),
                child: Text(context.l10n.onboardingRelayCustom),
              ),
            ),
          _field(context.l10n.onboardingRelayToken, _relayToken),
          _field(context.l10n.onboardingBind, _bind),
          _field(context.l10n.onboardingInternalToken, _internalToken),
          const SizedBox(height: 12),
          FilledButton(
            onPressed: _save,
            child: Text(context.l10n.onboardingSave),
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

  void _seedRelayOptions() {
    final set = <String>{};
    for (final url in _bootstrapRelays) {
      final opt = _relayOptionFromUrl(url);
      if (opt != null && set.add(opt.publicUrl)) {
        _relayOptions.add(opt);
      }
    }
    if (_relayOptions.isNotEmpty) {
      _selectedRelay = _relayOptions.first;
      _applyRelayOption(_selectedRelay!);
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
        final relays = (json['relays'] is List) ? json['relays'] : json['items'];
        if (relays is! List) continue;
        for (final item in relays) {
          if (item is! Map) continue;
          final base = (item['relay_base_url'] ?? item['relay_url'] ?? item['base'])?.toString().trim();
          if (base == null || base.isEmpty) continue;
          final ws = (item['relay_ws_url'] ?? item['relay_ws'] ?? item['ws'])?.toString().trim();
          final opt = _relayOptionFromParts(
            base,
            ws,
          );
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

  _RelayOption? _relayOptionFromParts(String base, String? ws) {
    final uri = Uri.tryParse(base);
    if (uri == null || uri.host.isEmpty) return null;
    final scheme = uri.scheme.isEmpty ? 'https' : uri.scheme;
    final publicUrl = uri.replace(scheme: scheme).toString().trim().replaceAll(RegExp(r'/$'), '');
    final wsUrl = (ws != null && ws.trim().isNotEmpty)
        ? ws.trim().replaceAll(RegExp(r'/$'), '')
        : (scheme == 'https' ? publicUrl.replaceFirst('https://', 'wss://') : publicUrl.replaceFirst('http://', 'ws://'));
    final domain = uri.host;
    return _RelayOption(
      publicUrl: publicUrl,
      wsUrl: wsUrl,
      domain: domain,
      label: uri.host,
    );
  }

  CoreConfig _buildConfig() {
    final username = _username.text.trim();
    final domain = _domain.text.trim();
    final announceHandle = domain.isNotEmpty ? '@announce@$domain' : '';
    final bootstrap = <String>[
      if (username.toLowerCase() == 'announce')
        ...const [
          '@redhunt07@www.foxyhole.io',
          '@engineering@newsmast.community',
          '@mullvadnet@mastodon.online',
          '@omgubuntu@floss.social',
          '@tassoman@misskey.social',
          '@informapirata@poliverso.org',
          '@lealternative@mastodon.uno',
          '@fsf@hostux.social',
          '@informapirata@mastodon.uno',
        ]
      else if (announceHandle.isNotEmpty)
        announceHandle,
    ];
    return CoreConfig(
      username: username,
      domain: domain,
      publicBaseUrl: _publicBaseUrl.text.trim(),
      relayWs: _relayWs.text.trim(),
      relayToken: _relayToken.text.trim(),
      bind: _bind.text.trim(),
      internalToken: _internalToken.text.trim(),
      apRelays: const [],
      bootstrapFollowActors: bootstrap,
      displayName: '',
      summary: '',
      iconUrl: '',
      iconMediaType: '',
      imageUrl: '',
      imageMediaType: '',
      profileFields: const [],
      manuallyApprovesFollowers: false,
      blockedDomains: const [],
      blockedActors: const [],
      upnpPortRangeStart: null,
      upnpPortRangeEnd: null,
    );
  }

  Future<void> _save() async {
    final relayToken = _relayToken.text.trim();
    if (relayToken.length < 16) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.onboardingRelayTokenTooShort)),
      );
      return;
    }
    final cfg = _buildConfig();
    await widget.appState.saveConfig(cfg);
    await widget.appState.startCore();
    if (!mounted) return;
    Navigator.of(context).popUntil((r) => r.isFirst);
  }

  Future<void> _importBackupFile() async {
    setState(() {
      _importing = true;
    });
    try {
      const group = XTypeGroup(
        label: 'JSON',
        extensions: ['json'],
        mimeTypes: ['application/json'],
        uniformTypeIdentifiers: ['public.json'],
      );
      final file = await openFile(acceptedTypeGroups: [group]);
      if (file == null) return;
      final raw = await file.readAsString();
      final bundle = BackupCodec.decode(raw);

      await widget.appState.stopCore();
      await widget.appState.saveConfig(bundle.config);
      await widget.appState.savePrefs(bundle.prefs);
      await widget.appState.startCore();

      if (!mounted) return;
      Navigator.of(context).popUntil((r) => r.isFirst);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.backupErr(e.toString()))),
      );
    } finally {
      if (mounted) {
        setState(() {
          _importing = false;
        });
      }
    }
  }

  Future<void> _importBackupCloud() async {
    setState(() {
      _importing = true;
    });
    try {
      final relayToken = _relayToken.text.trim();
      if (relayToken.length < 16) {
        throw StateError(context.l10n.onboardingRelayTokenTooShort);
      }
      final cfg = _buildConfig();
      final service = CloudBackupService(config: cfg);
      final pkg = await service.download();

      await widget.appState.stopCore();
      await widget.appState.saveConfig(pkg.config);
      await widget.appState.savePrefs(pkg.prefs);
      await widget.appState.startCore();

      final restoreService = CloudBackupService(config: pkg.config);
      await restoreService.restore(pkg);
      await widget.appState.stopCore();
      await widget.appState.startCore();

      if (!mounted) return;
      Navigator.of(context).popUntil((r) => r.isFirst);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(context.l10n.backupErr(e.toString()))),
      );
    } finally {
      if (mounted) {
        setState(() {
          _importing = false;
        });
      }
    }
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
