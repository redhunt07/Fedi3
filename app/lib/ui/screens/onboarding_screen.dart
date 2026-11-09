/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../model/relay_directory.dart';
import '../../services/call_ice_config.dart';
import '../../services/identity_sync.dart';
import '../../services/relay_directory_service.dart';
import '../../services/backup_codec.dart';
import '../../state/app_state.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/relay_latency_badge.dart';
import 'pairing_screen.dart';

enum OnboardingMode { loginExisting, createNew }

class OnboardingScreen extends StatefulWidget {
  const OnboardingScreen(
      {super.key,
      required this.appState,
      this.mode = OnboardingMode.createNew,
      this.initialRelay});

  final AppState appState;
  final OnboardingMode mode;

  /// Relay picked from the shared directory on the first-run screen.
  final RelayDirectoryEntry? initialRelay;

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
  late final TextEditingController _p2pRelayReserve;
  late final TextEditingController _webrtcIceUrls;
  late final TextEditingController _webrtcIceUsername;
  late final TextEditingController _webrtcIceCredential;
  late final TextEditingController _quickUrl;
  late final TextEditingController _quickToken;
  bool _manualLogin = false;
  bool _p2pEnable = true;
  bool _webrtcEnable = false;
  String _postDeliveryMode = CoreConfig.postDeliveryP2pRelay;
  bool _customRelay = false;
  String? _formError;
  bool _importing = false;
  bool _discovering = false;
  String? _relayDiscoveryError;
  final List<_RelayOption> _relayOptions = [];
  _RelayOption? _selectedRelay;

  @override
  void initState() {
    super.initState();
    final isCreate = widget.mode == OnboardingMode.createNew;
    _username = TextEditingController(text: isCreate ? 'alice' : '');
    _domain = TextEditingController(text: isCreate ? 'example.invalid' : '');
    _publicBaseUrl =
        TextEditingController(text: isCreate ? 'http://127.0.0.1:8787' : '');
    _relayWs =
        TextEditingController(text: isCreate ? 'ws://127.0.0.1:8787' : '');
    _relayToken = TextEditingController(text: CoreConfig.randomToken());
    _bind = TextEditingController(
        text: isCreate ? '127.0.0.1:8788' : '127.0.0.1:8788');
    _internalToken = TextEditingController(text: CoreConfig.randomToken());
    _p2pRelayReserve = TextEditingController();
    _webrtcIceUrls = TextEditingController();
    _webrtcIceUsername = TextEditingController();
    _webrtcIceCredential = TextEditingController();
    _quickUrl = TextEditingController(text: isCreate ? '' : '');
    _quickToken = TextEditingController(text: isCreate ? '' : '');
    _postDeliveryMode = CoreConfig.postDeliveryP2pRelay;
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
    _p2pRelayReserve.dispose();
    _webrtcIceUrls.dispose();
    _webrtcIceUsername.dispose();
    _webrtcIceCredential.dispose();
    _quickUrl.dispose();
    _quickToken.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(
          widget.mode == OnboardingMode.createNew
              ? context.l10n.onboardingCreateTitle
              : context.l10n.onboardingLoginTitle,
        ),
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          const Center(child: BrandMark(size: 64)),
          const SizedBox(height: 18),
          Text(
            widget.mode == OnboardingMode.createNew
                ? context.l10n.onboardingCreateTitle
                : context.l10n.onboardingLoginTitle,
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.headlineMedium,
          ),
          const SizedBox(height: 8),
          Text(
            widget.mode == OnboardingMode.createNew
                ? context.l10n.onboardingCreateIntro
                : context.l10n.onboardingLoginIntro,
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                ),
          ),
          const SizedBox(height: 20),
          if (widget.mode == OnboardingMode.loginExisting) ...[
            FilledButton.tonalIcon(
              onPressed: _importing ? null : _importBackupFile,
              icon: const Icon(Icons.upload_file),
              label: Text(context.l10n.onboardingImportBackup),
            ),
            const SizedBox(height: 16),
          ],
          GlassPanel(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _field(context.l10n.onboardingUsername, _username),
                if (widget.mode == OnboardingMode.loginExisting &&
                    !_manualLogin) ...[
                  _field(context.l10n.onboardingInstanceUrl, _quickUrl),
                  _field(context.l10n.onboardingRecoveryCode, _quickToken),
                  const SizedBox(height: 6),
                  AuroraButton(
                    label: context.l10n.onboardingConnect,
                    onPressed: _quickConnect,
                    icon: Icons.link_rounded,
                    expand: true,
                  ),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: Wrap(
                      spacing: 4,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        TextButton(
                          onPressed: () => setState(() => _manualLogin = true),
                          child: Text(context.l10n.onboardingManualConfig),
                        ),
                        TextButton.icon(
                          onPressed: _openPairing,
                          icon: const Icon(Icons.qr_code_2_rounded, size: 18),
                          label: Text(context.l10n.onboardingLoginWithQr),
                        ),
                      ],
                    ),
                  ),
                ] else ...[
                  _relayPicker(context),
                  if (_relayOptions.isNotEmpty && !_customRelay)
                    Align(
                      alignment: Alignment.centerLeft,
                      child: TextButton(
                        onPressed: () => setState(() => _customRelay = true),
                        child: Text(context.l10n.onboardingRelayCustom),
                      ),
                    ),
                  if (_customRelay ||
                      _selectedRelay == null ||
                      _relayOptions.isEmpty) ...[
                    _field(context.l10n.onboardingDomain, _domain),
                    _field(
                        context.l10n.onboardingRelayPublicUrl, _publicBaseUrl),
                    _field(context.l10n.onboardingRelayWs, _relayWs),
                  ],
                ],
              ],
            ),
          ),
          const SizedBox(height: 12),
          Card(
            child: ExpansionTile(
              shape: const Border(),
              collapsedShape: const Border(),
              leading: const Icon(Icons.tune_rounded),
              title: Text(context.l10n.onboardingAdvanced),
              childrenPadding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
              children: [
                _field(context.l10n.onboardingRelayToken, _relayToken),
                _field(context.l10n.onboardingBind, _bind),
                _field(context.l10n.onboardingInternalToken, _internalToken),
                _field(context.l10n.p2pRelayReserveLabel, _p2pRelayReserve,
                    maxLines: 3),
                SwitchListTile(
                  title: Text(context.l10n.onboardingEnableP2p),
                  value: _p2pEnable,
                  onChanged: (v) => setState(() {
                    _p2pEnable = v;
                    if (v) {
                      _webrtcEnable = true;
                      _autofillNetworkingDefaults(force: false);
                    }
                  }),
                ),
                DropdownButtonFormField<String>(
                  initialValue: _postDeliveryMode,
                  key: ValueKey(_postDeliveryMode),
                  decoration: InputDecoration(
                    labelText: context.l10n.postDeliveryModeLabel,
                  ),
                  items: [
                    DropdownMenuItem(
                      value: CoreConfig.postDeliveryP2pRelay,
                      child: Text(context.l10n.postDeliveryP2pRelay),
                    ),
                    DropdownMenuItem(
                      value: CoreConfig.postDeliveryP2pOnly,
                      child: Text(context.l10n.postDeliveryP2pOnly),
                    ),
                  ],
                  onChanged: (value) {
                    if (value == null) return;
                    setState(() => _postDeliveryMode = value);
                  },
                ),
                const SizedBox(height: 6),
                SwitchListTile(
                  title: Text(context.l10n.webrtcSectionTitle),
                  subtitle: Text(context.l10n.webrtcSectionHint),
                  value: _webrtcEnable,
                  onChanged: (v) => setState(() => _webrtcEnable = v),
                ),
                Align(
                  alignment: Alignment.centerLeft,
                  child: TextButton.icon(
                    onPressed: () => setState(
                        () => _autofillNetworkingDefaults(force: true)),
                    icon: const Icon(Icons.bolt),
                    label: Text(context.l10n.webrtcAutofill),
                  ),
                ),
                _field(context.l10n.webrtcIceUrlsLabel, _webrtcIceUrls,
                    maxLines: 3),
                _field(context.l10n.webrtcIceUsernameLabel, _webrtcIceUsername),
                _field(context.l10n.webrtcIceCredentialLabel,
                    _webrtcIceCredential),
              ],
            ),
          ),
          const SizedBox(height: 16),
          if (_formError != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 10),
              child: Text(
                _formError!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          AuroraButton(
            label: context.l10n.onboardingSave,
            onPressed: _save,
            icon: Icons.rocket_launch_rounded,
            expand: true,
          ),
        ],
      ),
    );
  }

  Widget _field(String label, TextEditingController controller,
      {int maxLines = 1}) {
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
          Text(context.l10n.onboardingRelaySelect,
              style: Theme.of(context).textTheme.labelLarge),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(
                child: DropdownButtonFormField<_RelayOption>(
                  // Long relay labels must ellipsize inside the row instead of
                  // forcing the field to its intrinsic (overflowing) width.
                  isExpanded: true,
                  initialValue: _selectedRelay,
                  items: _relayOptions
                      .map((r) => DropdownMenuItem(
                            value: r,
                            child:
                                Text(r.label, overflow: TextOverflow.ellipsis),
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
                icon: _discovering
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.public),
                label: Text(_discovering
                    ? context.l10n.onboardingRelayDiscovering
                    : context.l10n.onboardingRelayDiscover),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            context.l10n.relayDirectoryHint,
            style: TextStyle(
                color: Theme.of(context).colorScheme.onSurface.withAlpha(160),
                fontSize: 12),
          ),
          if (_relayDiscoveryError != null) ...[
            const SizedBox(height: 6),
            Text(
              _relayDiscoveryError!,
              style: TextStyle(
                  color: Theme.of(context).colorScheme.error, fontSize: 12),
            ),
          ],
        ],
      ),
    );
  }

  void _seedRelayOptions() {
    final set = <String>{};
    final initial = widget.initialRelay;
    if (initial != null) {
      final opt = _relayOptionFromEntry(initial);
      if (set.add(opt.publicUrl)) {
        _relayOptions.add(opt);
        _selectedRelay = opt;
        _applyRelayOption(opt);
      }
    }
    _loadCachedDirectory(set);
  }

  Future<void> _loadCachedDirectory(Set<String> seen) async {
    final snapshot = await RelayDirectoryService.cachedOrBundled();
    if (snapshot == null || !mounted) return;
    final cached = snapshot.entries
        .map(_relayOptionFromEntry)
        .where((opt) => seen.add(opt.publicUrl))
        .toList(growable: false);
    if (cached.isEmpty) return;
    setState(() => _relayOptions.addAll(cached));
  }

  Future<void> _discoverRelays() async {
    setState(() {
      _discovering = true;
      _relayDiscoveryError = null;
    });
    try {
      final snapshot = await RelayDirectoryService.refreshOrFallback();
      final latencies = await RelayDirectoryService.probeAll(snapshot.entries);
      final found = <_RelayOption>[];
      for (var i = 0; i < latencies.length; i++) {
        found.add(_relayOptionFromLatency(latencies[i], rank: i + 1));
      }
      if (found.isEmpty) {
        if (!mounted) return;
        setState(() {
          _relayDiscoveryError = context.l10n.relayDirectoryEmpty;
        });
        return;
      }
      if (!mounted) return;
      setState(() {
        _relayOptions
          ..clear()
          ..addAll(found);
        final reachable = found.where((r) => r.latencyMs != null).toList();
        _selectedRelay = reachable.isNotEmpty ? reachable.first : found.first;
        if (reachable.isNotEmpty) {
          _applyRelayOption(_selectedRelay!);
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _relayDiscoveryError = e.toString());
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

  _RelayOption _relayOptionFromEntry(RelayDirectoryEntry entry) {
    return _RelayOption(
      publicUrl: entry.url,
      wsUrl: entry.ws,
      domain: entry.host,
      label: entry.name,
    );
  }

  _RelayOption _relayOptionFromLatency(RelayLatency latency, {int? rank}) {
    final entry = latency.entry;
    final suffix = relayLatencyLabel(context, latency);
    return _RelayOption(
      publicUrl: entry.url,
      wsUrl: entry.ws,
      domain: entry.host,
      label: rank == null
          ? '${entry.host} — $suffix'
          : '#$rank ${entry.host} — $suffix',
      latencyMs: latency.latencyMs,
      failure: latency.failure,
      rank: rank,
    );
  }

  void _autofillNetworkingDefaults({required bool force}) {
    if (_webrtcEnable == false && _p2pEnable) {
      _webrtcEnable = true;
    }
    if (force || _webrtcIceUrls.text.trim().isEmpty) {
      _webrtcIceUrls.text = defaultCallIceUrls.join('\n');
    }
  }

  String? _validate() {
    final l10n = context.l10n;
    final username = _username.text.trim();
    if (!RegExp(r'^[A-Za-z0-9_]{1,64}$').hasMatch(username)) {
      return l10n.onboardingErrUsername;
    }
    final domain = _domain.text.trim();
    if (domain.isEmpty ||
        !RegExp(r'^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$')
            .hasMatch(domain) ||
        (!domain.contains('.') && domain != 'localhost')) {
      return l10n.onboardingErrDomain;
    }
    final publicBase = Uri.tryParse(_publicBaseUrl.text.trim());
    if (publicBase == null ||
        publicBase.host.isEmpty ||
        (publicBase.scheme != 'http' && publicBase.scheme != 'https')) {
      return l10n.onboardingErrPublicUrl;
    }
    final relayWs = Uri.tryParse(_relayWs.text.trim());
    if (relayWs == null ||
        relayWs.host.isEmpty ||
        (relayWs.scheme != 'ws' && relayWs.scheme != 'wss')) {
      return l10n.onboardingErrRelayWs;
    }
    final bind = Uri.tryParse('http://${_bind.text.trim()}');
    if (bind == null || bind.host.isEmpty || bind.port <= 0) {
      return l10n.onboardingErrBind;
    }
    return null;
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
    final validationError = _validate();
    if (validationError != null) {
      setState(() => _formError = validationError);
      return;
    }
    setState(() => _formError = null);
    _autofillNetworkingDefaults(force: false);
    var cfg = CoreConfig(
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
      webrtcIceUsername: _webrtcIceUsername.text.trim().isEmpty
          ? null
          : _webrtcIceUsername.text.trim(),
      webrtcIceCredential: _webrtcIceCredential.text.trim().isEmpty
          ? null
          : _webrtcIceCredential.text.trim(),
      apRelays: const [],
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
      postDeliveryMode: _postDeliveryMode,
      anchorActorUrl: widget.appState.config?.anchorActorUrl,
    );

    // Multi-device login: reuse the identity already stored by the relay.
    // A missing or unreadable blob must never block the login; only a core
    // that fails to start is a real error.
    var relayHasIdentity = false;
    if (widget.mode == OnboardingMode.loginExisting) {
      final relayBase = IdentitySync.relayHttpBaseFromWs(cfg.relayWs);
      if (relayBase.isNotEmpty) {
        try {
          final sealed = await IdentitySync.fetchRelayIdentity(
            relayBase: relayBase,
            user: cfg.username,
            token: relayToken,
          );
          if (sealed != null) {
            relayHasIdentity = true;
            if (IdentitySync.looksSealed(sealed)) {
              final package = await IdentitySync.open(sealed, relayToken);
              cfg = IdentitySync.mergeIdentity(cfg, package);
            }
          }
        } catch (_) {
          // Identity transfer is best-effort on login.
        }
      }
    }

    await widget.appState.saveConfig(cfg);
    await widget.appState.startCore();
    if (!mounted) return;
    if (!widget.appState.isRunning) {
      setState(() => _formError =
          widget.appState.lastError ?? context.l10n.networkErrorTitle);
      return;
    }
    final runningConfig = widget.appState.config;
    if (runningConfig != null && !relayHasIdentity) {
      // First client of the account: publish its identity for the others.
      // New accounts publish too, so a second device can log in right away.
      unawaited(IdentitySync.ensurePublished(runningConfig));
    }
    if (widget.mode == OnboardingMode.createNew) {
      await _showRecoveryCode(cfg);
      if (!mounted) return;
    }
    Navigator.of(context).popUntil((r) => r.isFirst);
  }

  Future<void> _openPairing() async {
    final entered = _quickUrl.text.trim().isNotEmpty
        ? _quickUrl.text.trim()
        : _publicBaseUrl.text.trim();
    final paired = await Navigator.of(context).push<bool>(
      MaterialPageRoute(
        builder: (_) => PairingScreen(
          appState: widget.appState,
          initialInstanceUrl: entered,
        ),
      ),
    );
    if (paired == true && mounted) {
      Navigator.of(context).popUntil((r) => r.isFirst);
    }
  }

  Future<void> _quickConnect() async {
    final raw = _quickUrl.text.trim();
    final uri = Uri.tryParse(raw);
    if (uri == null ||
        uri.host.isEmpty ||
        (uri.scheme != 'http' && uri.scheme != 'https')) {
      setState(() => _formError = context.l10n.onboardingErrPublicUrl);
      return;
    }
    final token = _quickToken.text.trim();
    if (token.length < 16) {
      setState(() => _formError = context.l10n.onboardingRelayTokenTooShort);
      return;
    }
    final base = raw.replaceAll(RegExp(r'/+$'), '');
    final wsScheme = uri.scheme == 'https' ? 'wss' : 'ws';
    final path = uri.path.replaceAll(RegExp(r'/+$'), '');
    _publicBaseUrl.text = base;
    _domain.text = uri.host;
    _relayWs.text = '$wsScheme://${uri.host}$path';
    _relayToken.text = token;
    _customRelay = true;
    setState(() => _formError = null);
    await _save();
  }

  Future<void> _showRecoveryCode(CoreConfig cfg) async {
    Widget codeRow(String label, String value) {
      return Padding(
        padding: const EdgeInsets.only(bottom: 10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(label,
                style: Theme.of(context).textTheme.labelSmall?.copyWith(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    )),
            const SizedBox(height: 4),
            Row(
              children: [
                Expanded(
                  child: SelectableText(
                    value,
                    style:
                        const TextStyle(fontFamily: 'monospace', fontSize: 12),
                  ),
                ),
                IconButton(
                  tooltip: context.l10n.onboardingCopy,
                  onPressed: () {
                    Clipboard.setData(ClipboardData(text: value));
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(content: Text(context.l10n.onboardingCopy)),
                    );
                  },
                  icon: const Icon(Icons.copy_rounded, size: 18),
                ),
              ],
            ),
          ],
        ),
      );
    }

    await showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) => AlertDialog(
        title: Text(context.l10n.onboardingRecoveryTitle),
        content: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(context.l10n.onboardingRecoveryHint),
              const SizedBox(height: 14),
              codeRow(context.l10n.onboardingRelayToken, cfg.relayToken),
              codeRow(context.l10n.onboardingInternalToken, cfg.internalToken),
            ],
          ),
        ),
        actions: [
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text(context.l10n.settingsOk),
          ),
        ],
      ),
    );
  }

  Future<void> _importBackupFile() async {
    setState(() {
      _importing = true;
    });
    try {
      final group = XTypeGroup(
        label: context.l10n.fileTypeJson,
        extensions: const ['json'],
        mimeTypes: const ['application/json'],
        uniformTypeIdentifiers: const ['public.json'],
      );
      final file = await openFile(acceptedTypeGroups: [group]);
      if (file == null) return;
      final raw = await file.readAsString();
      final bundle = BackupCodec.decode(raw);

      final prevCfg = widget.appState.config;
      final prevPrefs = widget.appState.prefs;
      await widget.appState.replaceConfig(bundle.config);
      try {
        await widget.appState.savePrefs(bundle.prefs);
      } catch (e) {
        await widget.appState.savePrefs(prevPrefs);
        if (prevCfg != null) await widget.appState.replaceConfig(prevCfg);
        rethrow;
      }

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
    this.latencyMs,
    this.failure,
    this.rank,
  });

  final String publicUrl;
  final String wsUrl;
  final String domain;
  final String label;
  final int? latencyMs;
  final RelayProbeFailure? failure;
  final int? rank;
}
