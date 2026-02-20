/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../model/ui_prefs.dart';
import '../../state/app_state.dart';
import 'emoji_palette_screen.dart';
import 'gif_settings_screen.dart';

class UiSettingsScreen extends StatelessWidget {
  const UiSettingsScreen({super.key, required this.appState});

  final AppState appState;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: appState,
      builder: (context, _) {
        final prefs = appState.prefs;
        return Scaffold(
          appBar: AppBar(title: Text(context.l10n.uiSettingsTitle)),
          body: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Card(
                child: ListTile(
                  title: Text(context.l10n.uiLanguage),
                  subtitle: Text(_localeLabel(context, prefs.localeTag)),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _pickLocale(context, prefs),
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(context.l10n.uiNotificationsTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
                      const SizedBox(height: 6),
                      SwitchListTile(
                        title: Text(context.l10n.uiNotificationsChat),
                        value: prefs.notifyChat,
                        onChanged: (v) => appState.savePrefs(prefs.copyWith(notifyChat: v)),
                      ),
                      SwitchListTile(
                        title: Text(context.l10n.uiNotificationsDirect),
                        value: prefs.notifyDirect,
                        onChanged: (v) => appState.savePrefs(prefs.copyWith(notifyDirect: v)),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.gifSettingsTitle),
                  subtitle: Text(context.l10n.gifSettingsHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(builder: (_) => GifSettingsScreen(appState: appState)),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.uiTheme),
                  subtitle: Text(_themeLabel(context, prefs.themeMode)),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _pickTheme(context, prefs),
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.uiDensity),
                  subtitle: Text(_densityLabel(context, prefs.density)),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _pickDensity(context, prefs),
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.uiAccent),
                  subtitle: Text(_accentLabel(prefs.accent)),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _pickAccent(context, prefs),
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: ListTile(
                  title: Text(context.l10n.uiEmojiPaletteTitle),
                  subtitle: Text(context.l10n.uiEmojiPaletteHint),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    Navigator.of(context).push(
                      MaterialPageRoute(builder: (_) => const EmojiPaletteScreen()),
                    );
                  },
                ),
              ),
              const SizedBox(height: 10),
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(context.l10n.uiEmojiPickerTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
                      const SizedBox(height: 10),
                      Text(context.l10n.uiEmojiPickerSizeLabel, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12)),
                      Slider(
                        value: prefs.emojiPickerScale.clamp(0.7, 1.6),
                        min: 0.7,
                        max: 1.6,
                        divisions: 9,
                        label: prefs.emojiPickerScale.toStringAsFixed(2),
                        onChanged: (v) => appState.savePrefs(prefs.copyWith(emojiPickerScale: v)),
                      ),
                      const SizedBox(height: 6),
                      Text(context.l10n.uiEmojiPickerColumnsLabel, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12)),
                      Slider(
                        value: prefs.emojiPickerColumns.toDouble().clamp(5, 12),
                        min: 5,
                        max: 12,
                        divisions: 7,
                        label: prefs.emojiPickerColumns.toString(),
                        onChanged: (v) => appState.savePrefs(prefs.copyWith(emojiPickerColumns: v.round())),
                      ),
                      const SizedBox(height: 6),
                      Text(context.l10n.uiEmojiPickerStyleLabel, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12)),
                      const SizedBox(height: 6),
                      SegmentedButton<EmojiPickerStyle>(
                        segments: [
                          ButtonSegment(value: EmojiPickerStyle.image, label: Text(context.l10n.uiEmojiPickerStyleImage)),
                          ButtonSegment(value: EmojiPickerStyle.text, label: Text(context.l10n.uiEmojiPickerStyleText)),
                        ],
                        selected: {prefs.emojiPickerStyle},
                        onSelectionChanged: (v) => appState.savePrefs(prefs.copyWith(emojiPickerStyle: v.first)),
                      ),
                      const SizedBox(height: 10),
                      Text(context.l10n.uiEmojiPickerPresetLabel, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12)),
                      const SizedBox(height: 6),
                      Wrap(
                        spacing: 8,
                        runSpacing: 6,
                        children: [
                          OutlinedButton(
                            onPressed: () => appState.savePrefs(prefs.copyWith(emojiPickerScale: 0.85, emojiPickerColumns: 10)),
                            child: Text(context.l10n.uiEmojiPickerPresetCompact),
                          ),
                          OutlinedButton(
                            onPressed: () => appState.savePrefs(prefs.copyWith(emojiPickerScale: 1.0, emojiPickerColumns: 8)),
                            child: Text(context.l10n.uiEmojiPickerPresetComfort),
                          ),
                          OutlinedButton(
                            onPressed: () => appState.savePrefs(prefs.copyWith(emojiPickerScale: 1.3, emojiPickerColumns: 6)),
                            child: Text(context.l10n.uiEmojiPickerPresetLarge),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Text(context.l10n.uiEmojiPickerPreviewLabel, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12)),
                      const SizedBox(height: 6),
                      LayoutBuilder(
                        builder: (context, constraints) {
                          const spacing = 8.0;
                          final columns = prefs.emojiPickerColumns.clamp(4, 12);
                          final size = ((constraints.maxWidth - spacing * (columns - 1)) / columns).clamp(32.0, 64.0);
                          final font = (20.0 * prefs.emojiPickerScale).clamp(16.0, 30.0);
                          const samples = ['😀', '😄', '😍', '🥳', '✨', '🔥', '👍', '❤️', '🙏', '👀', '😂', '🤔'];
                          return Wrap(
                            spacing: spacing,
                            runSpacing: spacing,
                            children: [
                              for (final e in samples)
                                Container(
                                  width: size,
                                  height: size,
                                  alignment: Alignment.center,
                                  decoration: BoxDecoration(
                                    color: Theme.of(context).colorScheme.surfaceContainerHighest,
                                    borderRadius: BorderRadius.circular(12),
                                  ),
                                  child: Text(e, style: TextStyle(fontSize: font)),
                                ),
                            ],
                          );
                        },
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
                      Text(context.l10n.uiFontSize, style: const TextStyle(fontWeight: FontWeight.w800)),
                      const SizedBox(height: 10),
                      Slider(
                        value: prefs.textScale.clamp(0.85, 1.25),
                        min: 0.85,
                        max: 1.25,
                        divisions: 8,
                        label: prefs.textScale.toStringAsFixed(2),
                        onChanged: (v) => appState.savePrefs(prefs.copyWith(textScale: v)),
                      ),
                      Text(
                        context.l10n.uiFontSizeHint,
                        style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12),
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

  String _localeLabel(BuildContext context, String tag) {
    final t = tag.trim();
    if (t.isEmpty) return context.l10n.uiLanguageSystem;
    if (t == 'it') return 'Italiano';
    if (t == 'en') return 'English';
    return t;
  }

  String _themeLabel(BuildContext context, UiThemeMode mode) {
    return switch (mode) {
      UiThemeMode.system => context.l10n.uiThemeSystem,
      UiThemeMode.light => context.l10n.uiThemeLight,
      UiThemeMode.dark => context.l10n.uiThemeDark,
    };
  }

  String _densityLabel(BuildContext context, UiDensity density) {
    return switch (density) {
      UiDensity.normal => context.l10n.uiDensityNormal,
      UiDensity.compact => context.l10n.uiDensityCompact,
    };
  }

  String _accentLabel(int argb) => '0x${argb.toRadixString(16).padLeft(8, '0').toUpperCase()}';

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
    await appState.savePrefs(prefs.copyWith(localeTag: value));
  }

  Future<void> _pickTheme(BuildContext context, UiPrefs prefs) async {
    final value = await showModalBottomSheet<UiThemeMode>(
      context: context,
      builder: (context) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            ListTile(title: Text(context.l10n.uiThemeSystem), onTap: () => Navigator.pop(context, UiThemeMode.system)),
            ListTile(title: Text(context.l10n.uiThemeLight), onTap: () => Navigator.pop(context, UiThemeMode.light)),
            ListTile(title: Text(context.l10n.uiThemeDark), onTap: () => Navigator.pop(context, UiThemeMode.dark)),
          ],
        ),
      ),
    );
    if (value == null) return;
    await appState.savePrefs(prefs.copyWith(themeMode: value));
  }

  Future<void> _pickDensity(BuildContext context, UiPrefs prefs) async {
    final value = await showModalBottomSheet<UiDensity>(
      context: context,
      builder: (context) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            ListTile(title: Text(context.l10n.uiDensityNormal), onTap: () => Navigator.pop(context, UiDensity.normal)),
            ListTile(title: Text(context.l10n.uiDensityCompact), onTap: () => Navigator.pop(context, UiDensity.compact)),
          ],
        ),
      ),
    );
    if (value == null) return;
    await appState.savePrefs(prefs.copyWith(density: value));
  }

  Future<void> _pickAccent(BuildContext context, UiPrefs prefs) async {
    const options = <int>[
      0xFF4AA8FF,
      0xFF7B61FF,
      0xFF00C2A8,
      0xFFFF5C5C,
      0xFFFFC542,
      0xFF00A2FF,
    ];
    final value = await showModalBottomSheet<int>(
      context: context,
      builder: (context) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              for (final c in options)
                InkWell(
                  onTap: () => Navigator.pop(context, c),
                  borderRadius: BorderRadius.circular(12),
                  child: Container(
                    width: 48,
                    height: 48,
                    decoration: BoxDecoration(
                      color: Color(c),
                      borderRadius: BorderRadius.circular(12),
                      border: Border.all(
                        color: Theme.of(context).colorScheme.onSurface.withAlpha(c == prefs.accent ? 200 : 80),
                        width: c == prefs.accent ? 2 : 1,
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
    if (value == null) return;
    await appState.savePrefs(prefs.copyWith(accent: value));
  }
}
