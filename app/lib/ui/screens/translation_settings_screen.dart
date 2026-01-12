/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/ui_prefs.dart';
import '../../state/app_state.dart';

class TranslationSettingsScreen extends StatefulWidget {
  const TranslationSettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<TranslationSettingsScreen> createState() => _TranslationSettingsScreenState();
}

class _TranslationSettingsScreenState extends State<TranslationSettingsScreen> {
  late final TextEditingController _authKeyCtrl = TextEditingController();
  late final TextEditingController _deeplxCtrl = TextEditingController();
  final FocusNode _authFocus = FocusNode();
  final FocusNode _deeplxFocus = FocusNode();

  @override
  void dispose() {
    _authKeyCtrl.dispose();
    _deeplxCtrl.dispose();
    _authFocus.dispose();
    _deeplxFocus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final prefs = widget.appState.prefs;
        if (!_authFocus.hasFocus && _authKeyCtrl.text != prefs.translationAuthKey) {
          _authKeyCtrl.text = prefs.translationAuthKey;
        }
        if (!_deeplxFocus.hasFocus && _deeplxCtrl.text != prefs.translationDeepLxUrl) {
          _deeplxCtrl.text = prefs.translationDeepLxUrl;
        }
        final target = _targetLabel(context, prefs.localeTag);
        final provider = prefs.translationProvider;

        return Scaffold(
          appBar: AppBar(title: Text(context.l10n.translationTitle)),
          body: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(context.l10n.translationProviderLabel, style: const TextStyle(fontWeight: FontWeight.w800)),
                      const SizedBox(height: 8),
                      SegmentedButton<TranslationProvider>(
                        segments: [
                          ButtonSegment(value: TranslationProvider.deepl, label: Text(context.l10n.translationProviderDeepL)),
                          ButtonSegment(value: TranslationProvider.deeplx, label: Text(context.l10n.translationProviderDeepLX)),
                        ],
                        selected: {provider},
                        onSelectionChanged: (v) {
                          widget.appState.savePrefs(prefs.copyWith(translationProvider: v.first));
                        },
                      ),
                      const SizedBox(height: 10),
                      Text(
                        context.l10n.translationTargetLabel(target),
                        style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160), fontSize: 12),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 10),
              if (provider == TranslationProvider.deepl) ...[
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(context.l10n.translationAuthKeyLabel, style: const TextStyle(fontWeight: FontWeight.w800)),
                        const SizedBox(height: 8),
                        TextField(
                          controller: _authKeyCtrl,
                          focusNode: _authFocus,
                          obscureText: true,
                          onChanged: (v) => widget.appState.savePrefs(prefs.copyWith(translationAuthKey: v.trim())),
                          decoration: InputDecoration(
                            hintText: context.l10n.translationAuthKeyHint,
                            border: const OutlineInputBorder(),
                          ),
                        ),
                        const SizedBox(height: 8),
                        SwitchListTile(
                          value: prefs.translationUsePro,
                          onChanged: (v) => widget.appState.savePrefs(prefs.copyWith(translationUsePro: v)),
                          title: Text(context.l10n.translationUseProLabel),
                          subtitle: Text(context.l10n.translationUseProHint),
                        ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 10),
              ],
              if (provider == TranslationProvider.deeplx) ...[
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(context.l10n.translationDeepLXUrlLabel, style: const TextStyle(fontWeight: FontWeight.w800)),
                        const SizedBox(height: 8),
                        TextField(
                          controller: _deeplxCtrl,
                          focusNode: _deeplxFocus,
                          onChanged: (v) => widget.appState.savePrefs(prefs.copyWith(translationDeepLxUrl: v.trim())),
                          decoration: InputDecoration(
                            hintText: context.l10n.translationDeepLXUrlHint,
                            border: const OutlineInputBorder(),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 10),
              ],
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(context.l10n.translationTimeoutLabel, style: const TextStyle(fontWeight: FontWeight.w800)),
                      const SizedBox(height: 8),
                      Slider(
                        value: prefs.translationTimeoutMs.toDouble().clamp(2000, 60000),
                        min: 2000,
                        max: 60000,
                        divisions: 29,
                        label: context.l10n.translationTimeoutValue((prefs.translationTimeoutMs / 1000).round()),
                        onChanged: (v) => widget.appState.savePrefs(prefs.copyWith(translationTimeoutMs: v.round())),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  String _targetLabel(BuildContext context, String tag) {
    final t = tag.trim();
    if (t.isNotEmpty) return t.toUpperCase();
    return Localizations.localeOf(context).languageCode.toUpperCase();
  }
}
