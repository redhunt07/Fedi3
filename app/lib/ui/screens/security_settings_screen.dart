/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../state/app_state.dart';

class SecuritySettingsScreen extends StatefulWidget {
  const SecuritySettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<SecuritySettingsScreen> createState() => _SecuritySettingsScreenState();
}

class _SecuritySettingsScreenState extends State<SecuritySettingsScreen> {
  late CoreConfig _cfg;
  late final TextEditingController _internalToken;
  bool _saving = false;
  String? _status;

  @override
  void initState() {
    super.initState();
    _cfg = widget.appState.config!;
    _internalToken = TextEditingController(text: _cfg.internalToken);
  }

  @override
  void dispose() {
    _internalToken.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.securityTitle),
        actions: [
          TextButton(onPressed: _saving ? null : _save, child: Text(context.l10n.save)),
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
          Text(context.l10n.securityInternalToken, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          TextField(
            controller: _internalToken,
            decoration: InputDecoration(hintText: context.l10n.securityInternalTokenHint),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              OutlinedButton.icon(
                onPressed: _saving
                    ? null
                    : () {
                        setState(() => _internalToken.text = CoreConfig.randomToken());
                      },
                icon: const Icon(Icons.autorenew),
                label: Text(context.l10n.securityRegenerate),
              ),
              OutlinedButton.icon(
                onPressed: _saving
                    ? null
                    : () async {
                        final messenger = ScaffoldMessenger.of(context);
                        final msg = context.l10n.copied;
                        final text = _internalToken.text.trim();
                        await Clipboard.setData(ClipboardData(text: text));
                        if (!mounted) return;
                        messenger.showSnackBar(SnackBar(content: Text(msg)));
                      },
                icon: const Icon(Icons.copy),
                label: Text(context.l10n.copy),
              ),
            ],
          ),
          const SizedBox(height: 16),
          Text(
            context.l10n.securityHintInternalEndpoints,
            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
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
      final token = _internalToken.text.trim();
      if (token.isEmpty) throw StateError('missing internal token');
      final c = _cfg;
      final updated = CoreConfig(
        username: c.username,
        domain: c.domain,
        publicBaseUrl: c.publicBaseUrl,
        relayWs: c.relayWs,
        relayToken: c.relayToken,
        bind: c.bind,
        internalToken: token,
        p2pEnable: c.p2pEnable,
        p2pRelayReserve: c.p2pRelayReserve,
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
        manuallyApprovesFollowers: c.manuallyApprovesFollowers,
        blockedDomains: c.blockedDomains,
        blockedActors: c.blockedActors,
        postDeliveryMode: c.postDeliveryMode,
        p2pCacheTtlSecs: c.p2pCacheTtlSecs,
        previousPublicBaseUrl: c.previousPublicBaseUrl,
        previousRelayToken: c.previousRelayToken,
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
