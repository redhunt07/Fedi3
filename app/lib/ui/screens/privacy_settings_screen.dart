/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';
import 'telemetry_screen.dart';

class PrivacySettingsScreen extends StatefulWidget {
  const PrivacySettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<PrivacySettingsScreen> createState() => _PrivacySettingsScreenState();
}

class _PrivacySettingsScreenState extends State<PrivacySettingsScreen> {
  late CoreConfig _cfg;
  bool _locked = false;
  bool _saving = false;
  String? _status;

  @override
  void initState() {
    super.initState();
    _cfg = widget.appState.config!;
    _locked = _cfg.manuallyApprovesFollowers;
  }

  @override
  Widget build(BuildContext context) {
    final prefs = widget.appState.prefs;
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.privacyTitle),
        actions: [
          TextButton(
            onPressed: _saving ? null : _save,
            child: Text(context.l10n.save),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(UiTokens.padScreen),
        children: [
          if (_status != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                _status!,
                style: TextStyle(
                    color: _status!.startsWith('OK')
                        ? null
                        : Theme.of(context).colorScheme.error),
              ),
            ),
          _section(context, context.l10n.privacyTitle),
          GlassPanel(
            child: Material(
              color: Colors.transparent,
              child: SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(context.l10n.privacyLockedAccount),
                subtitle: Text(context.l10n.privacyLockedAccountHint),
                value: _locked,
                onChanged: (v) => setState(() => _locked = v),
              ),
            ),
          ),
          _section(context, context.l10n.telemetrySectionTitle),
          GlassPanel(
            child: Column(
              children: [
                Material(
                  color: Colors.transparent,
                  child: SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(context.l10n.telemetryEnabled),
                    subtitle: Text(context.l10n.telemetryEnabledHint),
                    value: prefs.telemetryEnabled,
                    onChanged: (v) => widget.appState.updatePrefs(
                        (current) => current.copyWith(telemetryEnabled: v)),
                  ),
                ),
                Material(
                  color: Colors.transparent,
                  child: SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(context.l10n.telemetryMonitoringEnabled),
                    subtitle: Text(context.l10n.telemetryMonitoringHint),
                    value: prefs.clientMonitoringEnabled,
                    onChanged: (v) => widget.appState.updatePrefs((current) =>
                        current.copyWith(clientMonitoringEnabled: v)),
                  ),
                ),
                if (kDebugMode || prefs.clientMonitoringEnabled)
                  Align(
                    alignment: Alignment.centerLeft,
                    child: OutlinedButton(
                      onPressed: () {
                        Navigator.of(context).push(MaterialPageRoute(
                            builder: (_) => const TelemetryScreen()));
                      },
                      child: Text(context.l10n.telemetryOpen),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
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

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _status = null;
    });
    try {
      final c = _cfg;
      final updated = CoreConfig(
        username: c.username,
        domain: c.domain,
        publicBaseUrl: c.publicBaseUrl,
        relayWs: c.relayWs,
        relayToken: c.relayToken,
        bind: c.bind,
        internalToken: c.internalToken,
        p2pEnable: c.p2pEnable,
        p2pRelayReserve: c.p2pRelayReserve,
        p2pBootstrap: c.p2pBootstrap,
        p2pPublicDiscovery: c.p2pPublicDiscovery,
        p2pPeerExchange: c.p2pPeerExchange,
        webrtcEnable: c.webrtcEnable,
        webrtcIceUrls: c.webrtcIceUrls,
        webrtcIceUsername: c.webrtcIceUsername,
        webrtcIceCredential: c.webrtcIceCredential,
        apRelays: c.apRelays,
        displayName: c.displayName,
        summary: c.summary,
        iconUrl: c.iconUrl,
        iconMediaType: c.iconMediaType,
        imageUrl: c.imageUrl,
        imageMediaType: c.imageMediaType,
        profileFields: c.profileFields,
        manuallyApprovesFollowers: _locked,
        blockedDomains: c.blockedDomains,
        blockedActors: c.blockedActors,
        postDeliveryMode: c.postDeliveryMode,
        p2pCacheTtlSecs: c.p2pCacheTtlSecs,
        previousPublicBaseUrl: c.previousPublicBaseUrl,
        previousRelayToken: c.previousRelayToken,
        anchorActorUrl: c.anchorActorUrl,
      );
      await widget.appState.replaceConfig(updated);
      if (!mounted) return;
      setState(() => _status = context.l10n.ok);
    } catch (e) {
      setState(() => _status = context.l10n.err(e.toString()));
    } finally {
      setState(() => _saving = false);
    }
  }
}
