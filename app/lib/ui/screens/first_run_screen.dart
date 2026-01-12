/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'dart:convert';

import '../../l10n/l10n_ext.dart';
import '../../model/ui_prefs.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
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
  final List<String> _relayPreview = [];
  final List<String> _peerPreview = [];

  static const List<String> _bootstrapRelays = [
    'https://relay.fedi3.com',
  ];

  @override
  void initState() {
    super.initState();
    _loadPreview();
  }

  Future<void> _loadPreview() async {
    setState(() {
      _loadingPreview = true;
      _previewError = null;
      _relayPreview.clear();
      _peerPreview.clear();
    });
    try {
      final relaySet = <String>{};
      final peerSet = <String>{};
      for (final base in _bootstrapRelays) {
        final relaysUri = Uri.parse(base).replace(path: '/_fedi3/relay/relays');
        final relaysResp = await http.get(relaysUri);
        if (relaysResp.statusCode >= 200 && relaysResp.statusCode < 300) {
          final json = jsonDecode(relaysResp.body);
          if (json is Map) {
            final relays = json['relays'];
            if (relays is List) {
              for (final item in relays) {
                if (item is Map) {
                  final url = (item['relay_url'] as String?)?.trim();
                  if (url != null && url.isNotEmpty) {
                    relaySet.add(url);
                  }
                }
              }
            }
          }
        }
        final peersUri = Uri.parse(base).replace(path: '/_fedi3/relay/peers');
        final peersResp = await http.get(peersUri);
        if (peersResp.statusCode >= 200 && peersResp.statusCode < 300) {
          final json = jsonDecode(peersResp.body);
          if (json is Map) {
            final items = json['items'];
            if (items is List) {
              for (final item in items) {
                if (item is Map) {
                  final name = (item['username'] as String?)?.trim();
                  if (name != null && name.isNotEmpty) {
                    peerSet.add(name);
                  }
                }
              }
            }
          }
        }
      }
      if (!mounted) return;
      setState(() {
        _relayPreview
          ..clear()
          ..addAll(relaySet.take(5));
        _peerPreview
          ..clear()
          ..addAll(peerSet.take(5));
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _previewError = e.toString());
    } finally {
      if (mounted) {
        setState(() => _loadingPreview = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final prefs = widget.appState.prefs;
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
                        builder: (_) => OnboardingScreen(appState: widget.appState, mode: OnboardingMode.loginExisting),
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
                        builder: (_) => OnboardingScreen(appState: widget.appState, mode: OnboardingMode.createNew),
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
            ListTile(title: Text(context.l10n.uiLanguageSystem), onTap: () => Navigator.pop(context, '')),
            const Divider(height: 1),
            ListTile(title: const Text('English'), onTap: () => Navigator.pop(context, 'en')),
            ListTile(title: const Text('Italiano'), onTap: () => Navigator.pop(context, 'it')),
          ],
        ),
      ),
    );
    if (value == null) return;
    await widget.appState.savePrefs(prefs.copyWith(localeTag: value));
  }

  Widget _networkPreviewCard(BuildContext context) {
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
                Text(context.l10n.firstRunRelayPreviewTitle, style: const TextStyle(fontWeight: FontWeight.w700)),
                const Spacer(),
                IconButton(
                  onPressed: _loadingPreview ? null : _loadPreview,
                  icon: _loadingPreview
                      ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.refresh),
                ),
              ],
            ),
            if (_previewError != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: NetworkErrorCard(
                  message: _previewError ?? context.l10n.firstRunRelayPreviewError,
                  onRetry: _loadPreview,
                  compact: true,
                ),
              ),
            if (_previewError == null) ...[
              const SizedBox(height: 6),
              Text(context.l10n.firstRunRelayPreviewRelays),
              const SizedBox(height: 4),
              if (_relayPreview.isEmpty)
                Text(
                  context.l10n.firstRunRelayPreviewEmpty,
                  style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
                )
              else
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: _relayPreview.map((r) => Chip(label: Text(r))).toList(),
                ),
              const SizedBox(height: 8),
              Text(context.l10n.firstRunRelayPreviewPeers),
              const SizedBox(height: 4),
              if (_peerPreview.isEmpty)
                Text(
                  context.l10n.firstRunRelayPreviewEmpty,
                  style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
                )
              else
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: _peerPreview.map((p) => Chip(label: Text(p))).toList(),
                ),
            ],
          ],
        ),
      ),
    );
  }
}
