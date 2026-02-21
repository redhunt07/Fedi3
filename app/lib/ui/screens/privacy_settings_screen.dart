/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../state/app_state.dart';
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
        padding: const EdgeInsets.all(16),
        children: [
          if (_status != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                _status!,
                style: TextStyle(color: _status!.startsWith('OK') ? null : Theme.of(context).colorScheme.error),
              ),
            ),
          SwitchListTile(
            title: Text(context.l10n.privacyLockedAccount),
            subtitle: Text(context.l10n.privacyLockedAccountHint),
            value: _locked,
            onChanged: (v) => setState(() => _locked = v),
          ),
          const SizedBox(height: 12),
          Text(context.l10n.telemetrySectionTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          SwitchListTile(
            title: Text(context.l10n.telemetryEnabled),
            subtitle: Text(context.l10n.telemetryEnabledHint),
            value: prefs.telemetryEnabled,
            onChanged: (v) => widget.appState.savePrefs(prefs.copyWith(telemetryEnabled: v)),
          ),
          SwitchListTile(
            title: Text(context.l10n.telemetryMonitoringEnabled),
            subtitle: Text(context.l10n.telemetryMonitoringHint),
            value: prefs.clientMonitoringEnabled,
            onChanged: (v) => widget.appState.savePrefs(prefs.copyWith(clientMonitoringEnabled: v)),
          ),
          if (kDebugMode || prefs.clientMonitoringEnabled)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: OutlinedButton(
                onPressed: () {
                  Navigator.of(context).push(MaterialPageRoute(builder: (_) => const TelemetryScreen()));
                },
                child: Text(context.l10n.telemetryOpen),
              ),
            ),
        ],
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
        apRelays: c.apRelays,
        bootstrapFollowActors: c.bootstrapFollowActors,
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
        previousPublicBaseUrl: c.previousPublicBaseUrl,
        previousRelayToken: c.previousRelayToken,
        upnpPortRangeStart: c.upnpPortRangeStart,
        upnpPortRangeEnd: c.upnpPortRangeEnd,
        upnpLeaseSecs: c.upnpLeaseSecs,
        upnpTimeoutSecs: c.upnpTimeoutSecs,
        postDeliveryMode: c.postDeliveryMode,
        p2pRelayFallbackSecs: c.p2pRelayFallbackSecs,
        p2pCacheTtlSecs: c.p2pCacheTtlSecs,
      );
      await widget.appState.stopCore();
      await widget.appState.saveConfig(updated);
      await widget.appState.startCore();
      if (!mounted) return;
      setState(() => _status = context.l10n.ok);
    } catch (e) {
      setState(() => _status = context.l10n.err(e.toString()));
    } finally {
      setState(() => _saving = false);
    }
  }
}
