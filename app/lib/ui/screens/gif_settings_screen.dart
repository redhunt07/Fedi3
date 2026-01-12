/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';

class GifSettingsScreen extends StatefulWidget {
  const GifSettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<GifSettingsScreen> createState() => _GifSettingsScreenState();
}

class _GifSettingsScreenState extends State<GifSettingsScreen> {
  static const String _defaultGiphyKey = 'JqCEl4nBczyPxQPX7ooxoQIzsKhnsi2e';
  late final TextEditingController _keyCtrl = TextEditingController();
  final FocusNode _keyFocus = FocusNode();

  @override
  void dispose() {
    _keyCtrl.dispose();
    _keyFocus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: widget.appState,
      builder: (context, _) {
        final prefs = widget.appState.prefs;
        if (!_keyFocus.hasFocus && _keyCtrl.text != prefs.gifApiKey) {
          _keyCtrl.text = prefs.gifApiKey;
        }
        return Scaffold(
          appBar: AppBar(title: Text(context.l10n.gifSettingsTitle)),
          body: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        context.l10n.gifProviderHint,
                        style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160), fontSize: 12),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(context.l10n.gifApiKeyLabel, style: const TextStyle(fontWeight: FontWeight.w800)),
                      const SizedBox(height: 8),
                      TextField(
                        controller: _keyCtrl,
                        focusNode: _keyFocus,
                        obscureText: true,
                        onChanged: (v) => widget.appState.savePrefs(prefs.copyWith(gifApiKey: v.trim())),
                        decoration: InputDecoration(
                          hintText: context.l10n.gifApiKeyHint,
                          border: const OutlineInputBorder(),
                        ),
                      ),
                      if (prefs.gifApiKey.trim().isEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 8),
                          child: Text(
                            context.l10n.gifSettingsDefaultHint,
                            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(150), fontSize: 12),
                          ),
                        ),
                      if (prefs.gifApiKey.trim().isEmpty)
                        TextButton(
                          onPressed: () => widget.appState.savePrefs(prefs.copyWith(gifApiKey: _defaultGiphyKey)),
                          child: Text(context.l10n.gifSettingsUseDefault),
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
}
