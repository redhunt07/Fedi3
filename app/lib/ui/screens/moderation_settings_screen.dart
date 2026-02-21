/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../state/app_state.dart';

class ModerationSettingsScreen extends StatefulWidget {
  const ModerationSettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<ModerationSettingsScreen> createState() => _ModerationSettingsScreenState();
}

class _ModerationSettingsScreenState extends State<ModerationSettingsScreen> {
  late CoreConfig _cfg;
  late final TextEditingController _blockedDomains;
  late final TextEditingController _blockedActors;
  bool _saving = false;
  String? _status;

  @override
  void initState() {
    super.initState();
    _cfg = widget.appState.config!;
    _blockedDomains = TextEditingController(text: _cfg.blockedDomains.join('\n'));
    _blockedActors = TextEditingController(text: _cfg.blockedActors.join('\n'));
  }

  @override
  void dispose() {
    _blockedDomains.dispose();
    _blockedActors.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.moderationTitle),
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
          Text(context.l10n.moderationBlockedDomains, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          TextField(
            controller: _blockedDomains,
            maxLines: 6,
            decoration: InputDecoration(hintText: context.l10n.moderationBlockedDomainsHint),
          ),
          const SizedBox(height: 16),
          Text(context.l10n.moderationBlockedActors, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          TextField(
            controller: _blockedActors,
            maxLines: 6,
            decoration: InputDecoration(hintText: context.l10n.moderationBlockedActorsHint),
          ),
          const SizedBox(height: 16),
          Text(
            context.l10n.moderationHint,
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
      final domains = _blockedDomains.text
          .split('\n')
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toSet()
          .toList();
      final actors = _blockedActors.text
          .split('\n')
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toSet()
          .toList();

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
        manuallyApprovesFollowers: c.manuallyApprovesFollowers,
        blockedDomains: domains,
        blockedActors: actors,
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
