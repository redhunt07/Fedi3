/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'dart:convert';

import 'package:url_launcher/url_launcher.dart';

import '../../core/core_http.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/relay_directory.dart';
import '../../model/ui_prefs.dart';
import '../../services/relay_directory_service.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/relay_latency_badge.dart';
import 'onboarding_screen.dart';

class FirstRunScreen extends StatefulWidget {
  const FirstRunScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<FirstRunScreen> createState() => _FirstRunScreenState();
}

class _FirstRunScreenState extends State<FirstRunScreen> {
  bool _loadingPreview = false;
  String? _previewError;
  List<RelayLatency> _relayLatencies = const [];
  List<String> _peerPreview = const [];
  DateTime? _measuredAt;

  @override
  void initState() {
    super.initState();
    if (!widget.appState.configCorrupt &&
        !widget.appState.credentialsUnavailable) {
      _loadPreview();
    }
  }

  Future<void> _loadPreview() async {
    setState(() {
      _loadingPreview = true;
      _previewError = null;
    });
    try {
      // Show the local copy immediately (cache or bundled asset), then refresh
      // from the network so a slow or offline GitHub does not block the screen.
      var snapshot = await RelayDirectoryService.cachedOrBundled();
      Object? fetchError;
      if (snapshot != null) {
        await _applySnapshot(snapshot);
      }
      try {
        snapshot = await RelayDirectoryService.fetch();
      } catch (e) {
        fetchError = e;
      }
      if (snapshot != null) {
        await _applySnapshot(snapshot);
      } else if (_relayLatencies.isEmpty) {
        throw fetchError ?? StateError('relay directory unavailable');
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _previewError = e.toString());
    } finally {
      if (mounted) {
        setState(() => _loadingPreview = false);
      }
    }
  }

  Future<void> _applySnapshot(RelayDirectorySnapshot snapshot) async {
    final latencies = await RelayDirectoryService.probeAll(snapshot.entries);
    final peers = <String>[];
    RelayLatency? fastest;
    for (final latency in latencies) {
      if (latency.reachable) {
        fastest = latency;
        break;
      }
    }
    if (fastest != null) {
      peers.addAll(await _fetchPeers(fastest.entry.url));
    }
    if (!mounted) return;
    setState(() {
      _relayLatencies = latencies;
      _peerPreview = peers.take(5).toList(growable: false);
      _measuredAt = DateTime.now();
    });
  }

  Future<List<String>> _fetchPeers(String base) async {
    final out = <String>[];
    try {
      final uri = Uri.parse('$base/_fedi3/relay/peers');
      final resp =
          await CoreHttp.client.get(uri).timeout(CoreHttp.totalTimeout);
      if (resp.statusCode < 200 || resp.statusCode >= 300) return out;
      final json = jsonDecode(resp.body);
      if (json is! Map) return out;
      final items = json['items'];
      if (items is! List) return out;
      for (final item in items) {
        if (item is! Map) continue;
        final name = (item['username'] as String?)?.trim();
        if (name != null && name.isNotEmpty && !out.contains(name)) {
          out.add(name);
        }
      }
    } catch (_) {
      // Peer preview is best-effort; a relay can be healthy without peers.
    }
    return out;
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final prefs = widget.appState.prefs;
        if (widget.appState.configCorrupt ||
            widget.appState.credentialsUnavailable) {
          return Scaffold(
            appBar: AppBar(title: Text(context.l10n.configRecoveryTitle)),
            body: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Icon(Icons.lock_outline, size: 36),
                      const SizedBox(height: 16),
                      Text(widget.appState.credentialsUnavailable
                          ? context.l10n.keyringUnavailableHint
                          : context.l10n.corruptConfigHint),
                      const SizedBox(height: 16),
                      FilledButton(
                          onPressed: widget.appState.reloadConfig,
                          child: Text(context.l10n.networkErrorRetry)),
                    ])),
          );
        }
        return Scaffold(
          appBar: AppBar(title: Text(context.l10n.firstRunTitle)),
          body: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Text(context.l10n.firstRunIntro),
              const SizedBox(height: 16),
              Card(
                child: ListTile(
                  leading: const Icon(Icons.language),
                  title: Text(context.l10n.uiLanguage),
                  subtitle: Text(_localeLabel(context, prefs.localeTag)),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _pickLocale(context, prefs),
                ),
              ),
              const SizedBox(height: 16),
              _networkPreviewCard(context),
              const SizedBox(height: 16),
              Card(
                child: ListTile(
                  leading: const Icon(Icons.login),
                  title: Text(context.l10n.firstRunLoginTitle),
                  subtitle: Text(context.l10n.firstRunLoginHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => OnboardingScreen(
                            appState: widget.appState,
                            mode: OnboardingMode.loginExisting),
                      ),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  leading: const Icon(Icons.person_add_alt_1),
                  title: Text(context.l10n.firstRunCreateTitle),
                  subtitle: Text(context.l10n.firstRunCreateHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(
                        builder: (_) => OnboardingScreen(
                            appState: widget.appState,
                            mode: OnboardingMode.createNew),
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  String _localeLabel(BuildContext context, String tag) {
    final t = tag.trim();
    if (t.isEmpty) return context.l10n.uiLanguageSystem;
    if (t == 'it') return 'Italiano';
    if (t == 'en') return 'English';
    return t;
  }

  Future<void> _pickLocale(BuildContext context, UiPrefs prefs) async {
    final value = await showModalBottomSheet<String>(
      context: context,
      builder: (context) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            ListTile(
                title: Text(context.l10n.uiLanguageSystem),
                onTap: () => Navigator.pop(context, '')),
            const Divider(height: 1),
            ListTile(
                title: const Text('English'),
                onTap: () => Navigator.pop(context, 'en')),
            ListTile(
                title: const Text('Italiano'),
                onTap: () => Navigator.pop(context, 'it')),
          ],
        ),
      ),
    );
    if (value == null) return;
    await widget.appState
        .updatePrefs((current) => current.copyWith(localeTag: value));
  }

  Widget _networkPreviewCard(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final muted = scheme.onSurface.withAlpha(160);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.public),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(context.l10n.firstRunRelayPreviewTitle,
                      style: const TextStyle(fontWeight: FontWeight.w700)),
                ),
                IconButton(
                  tooltip: context.l10n.relayDirectoryRefresh,
                  onPressed: _loadingPreview ? null : _loadPreview,
                  icon: _loadingPreview
                      ? const Icon(Icons.hourglass_top)
                      : const Icon(Icons.refresh),
                ),
              ],
            ),
            const SizedBox(height: 2),
            Text(context.l10n.relayDirectoryHint,
                style: TextStyle(color: muted, fontSize: 12)),
            if (_loadingPreview && _relayLatencies.isEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 10),
                child: Row(children: [
                  Icon(Icons.hourglass_top, size: 14, color: muted),
                  const SizedBox(width: 8),
                  Text(context.l10n.relayDirectoryMeasuring,
                      style: TextStyle(color: muted, fontSize: 12)),
                ]),
              ),
            if (_previewError != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: NetworkErrorCard(
                  message:
                      _previewError ?? context.l10n.firstRunRelayPreviewError,
                  onRetry: _loadPreview,
                  compact: true,
                ),
              ),
            if (_previewError == null) ...[
              const SizedBox(height: 8),
              if (_relayLatencies.isEmpty)
                Text(context.l10n.relayDirectoryEmpty,
                    style: TextStyle(color: muted))
              else
                for (var i = 0; i < _relayLatencies.length; i++)
                  _relayRow(context, i + 1, _relayLatencies[i]),
              const SizedBox(height: 4),
              Row(
                children: [
                  if (_measuredAt != null)
                    Expanded(
                      child: Text(
                        context.l10n
                            .relayLatencyMeasuredAt(_formatTime(_measuredAt!)),
                        style: TextStyle(color: muted, fontSize: 11),
                      ),
                    ),
                  TextButton.icon(
                    onPressed: () => launchUrl(
                        Uri.parse(RelayDirectory.submitUrl),
                        mode: LaunchMode.externalApplication),
                    icon: const Icon(Icons.add, size: 16),
                    label: Text(context.l10n.relayDirectoryAddYours),
                  ),
                ],
              ),
              const Divider(height: 20),
              Text(context.l10n.firstRunRelayPreviewPeers,
                  style: const TextStyle(fontWeight: FontWeight.w600)),
              const SizedBox(height: 4),
              if (_peerPreview.isEmpty)
                Text(context.l10n.firstRunRelayPreviewEmpty,
                    style: TextStyle(color: muted))
              else
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children:
                      _peerPreview.map((p) => Chip(label: Text(p))).toList(),
                ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _relayRow(BuildContext context, int rank, RelayLatency latency) {
    final scheme = Theme.of(context).colorScheme;
    final entry = latency.entry;
    return InkWell(
      borderRadius: BorderRadius.circular(10),
      onTap: () {
        Navigator.of(context).push(
          MaterialPageRoute(
            builder: (_) => OnboardingScreen(
              appState: widget.appState,
              mode: OnboardingMode.createNew,
              initialRelay: entry,
            ),
          ),
        );
      },
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: Row(
          children: [
            SizedBox(
              width: 26,
              child: Text('#$rank',
                  style: TextStyle(
                      color: scheme.onSurfaceVariant,
                      fontSize: 11,
                      fontWeight: FontWeight.w700)),
            ),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(entry.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontWeight: FontWeight.w600)),
                  Text(entry.host,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          color: scheme.onSurfaceVariant, fontSize: 11)),
                ],
              ),
            ),
            const SizedBox(width: 8),
            RelayLatencyBadge(latency: latency),
          ],
        ),
      ),
    );
  }

  String _formatTime(DateTime value) {
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(value.hour)}:${two(value.minute)}:${two(value.second)}';
  }
}
