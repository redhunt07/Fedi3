/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

enum UiThemeMode { system, light, dark }

enum UiDensity { normal, compact }

enum EmojiPickerStyle { image, text }

enum TranslationProvider { deepl, deeplx }

class UiPrefs {
  static const String _legacyTenorKey = 'AIzaSyBX2FXmLsYCAG7oucPMsbWSfGzaWD2bLnM';
  static const String _defaultGiphyKey = 'JqCEl4nBczyPxQPX7ooxoQIzsKhnsi2e';

  const UiPrefs({
    required this.themeMode,
    required this.density,
    required this.accent,
    required this.textScale,
    required this.localeTag,
    required this.lastNotificationsSeenMs,
    required this.lastChatSeenMs,
    required this.chatThreadSeenMs,
    required this.pinnedChatThreads,
    required this.desktopUseColumns,
    required this.mediaAutoplay,
    required this.mediaMuted,
    required this.emojiPickerScale,
    required this.emojiPickerColumns,
    required this.emojiPickerStyle,
    required this.gifApiKey,
    required this.translationProvider,
    required this.translationAuthKey,
    required this.translationUsePro,
    required this.translationTimeoutMs,
    required this.translationDeepLxUrl,
    required this.telemetryEnabled,
    required this.clientMonitoringEnabled,
    required this.notifyChat,
    required this.notifyDirect,
    required this.notifyMutedUntilMs,
    required this.relayAdminToken,
    required this.useTor,
    required this.proxyHost,
    required this.proxyPort,
    required this.proxyType,
  });

  final UiThemeMode themeMode;
  final UiDensity density;
  final int accent; // ARGB int
  final double textScale; // 0.85 .. 1.25
  final String localeTag; // '', 'en', 'it'
  final int lastNotificationsSeenMs;
  final int lastChatSeenMs;
  final Map<String, int> chatThreadSeenMs;
  final List<String> pinnedChatThreads;
  final bool desktopUseColumns;
  final bool mediaAutoplay;
  final bool mediaMuted;
  final double emojiPickerScale;
  final int emojiPickerColumns;
  final EmojiPickerStyle emojiPickerStyle;
  final String gifApiKey;
  final TranslationProvider translationProvider;
  final String translationAuthKey;
  final bool translationUsePro;
  final int translationTimeoutMs;
  final String translationDeepLxUrl;
  final bool telemetryEnabled;
  final bool clientMonitoringEnabled;
  final bool notifyChat;
  final bool notifyDirect;
  final int notifyMutedUntilMs;
  final String relayAdminToken;
  final bool useTor;
  final String? proxyHost;
  final int? proxyPort;
  final String? proxyType;

  static UiPrefs defaults() => const UiPrefs(
        themeMode: UiThemeMode.system,
        density: UiDensity.normal,
        accent: 0xFF4AA8FF,
        textScale: 1.0,
        localeTag: '',
        lastNotificationsSeenMs: 0,
        lastChatSeenMs: 0,
        chatThreadSeenMs: {},
        pinnedChatThreads: [],
        desktopUseColumns: true,
        mediaAutoplay: true,
        mediaMuted: false,
        emojiPickerScale: 1.0,
        emojiPickerColumns: 8,
        emojiPickerStyle: EmojiPickerStyle.image,
        gifApiKey: _defaultGiphyKey,
        translationProvider: TranslationProvider.deepl,
        translationAuthKey: '',
        translationUsePro: false,
        translationTimeoutMs: 10000,
        translationDeepLxUrl: 'https://api.deeplx.org/translate',
        telemetryEnabled: false,
        clientMonitoringEnabled: false,
        notifyChat: true,
        notifyDirect: true,
        notifyMutedUntilMs: 0,
        relayAdminToken: '',
        useTor: false,
        proxyHost: null,
        proxyPort: null,
        proxyType: null,
      );

  UiPrefs copyWith({
    UiThemeMode? themeMode,
    UiDensity? density,
    int? accent,
    double? textScale,
    String? localeTag,
    int? lastNotificationsSeenMs,
    int? lastChatSeenMs,
    Map<String, int>? chatThreadSeenMs,
    List<String>? pinnedChatThreads,
    bool? desktopUseColumns,
    bool? mediaAutoplay,
    bool? mediaMuted,
    double? emojiPickerScale,
    int? emojiPickerColumns,
    EmojiPickerStyle? emojiPickerStyle,
    String? gifApiKey,
    TranslationProvider? translationProvider,
    String? translationAuthKey,
    bool? translationUsePro,
    int? translationTimeoutMs,
    String? translationDeepLxUrl,
    bool? telemetryEnabled,
    bool? clientMonitoringEnabled,
    bool? notifyChat,
    bool? notifyDirect,
    int? notifyMutedUntilMs,
    String? relayAdminToken,
    bool? useTor,
    String? proxyHost,
    int? proxyPort,
    String? proxyType,
  }) {
    return UiPrefs(
      themeMode: themeMode ?? this.themeMode,
      density: density ?? this.density,
      accent: accent ?? this.accent,
      textScale: textScale ?? this.textScale,
      localeTag: localeTag ?? this.localeTag,
      lastNotificationsSeenMs: lastNotificationsSeenMs ?? this.lastNotificationsSeenMs,
      lastChatSeenMs: lastChatSeenMs ?? this.lastChatSeenMs,
      chatThreadSeenMs: chatThreadSeenMs ?? this.chatThreadSeenMs,
      pinnedChatThreads: pinnedChatThreads ?? this.pinnedChatThreads,
      desktopUseColumns: desktopUseColumns ?? this.desktopUseColumns,
      mediaAutoplay: mediaAutoplay ?? this.mediaAutoplay,
      mediaMuted: mediaMuted ?? this.mediaMuted,
      emojiPickerScale: emojiPickerScale ?? this.emojiPickerScale,
      emojiPickerColumns: emojiPickerColumns ?? this.emojiPickerColumns,
      emojiPickerStyle: emojiPickerStyle ?? this.emojiPickerStyle,
      gifApiKey: gifApiKey ?? this.gifApiKey,
      translationProvider: translationProvider ?? this.translationProvider,
      translationAuthKey: translationAuthKey ?? this.translationAuthKey,
      translationUsePro: translationUsePro ?? this.translationUsePro,
      translationTimeoutMs: translationTimeoutMs ?? this.translationTimeoutMs,
      translationDeepLxUrl: translationDeepLxUrl ?? this.translationDeepLxUrl,
      telemetryEnabled: telemetryEnabled ?? this.telemetryEnabled,
      clientMonitoringEnabled: clientMonitoringEnabled ?? this.clientMonitoringEnabled,
      notifyChat: notifyChat ?? this.notifyChat,
      notifyDirect: notifyDirect ?? this.notifyDirect,
      notifyMutedUntilMs: notifyMutedUntilMs ?? this.notifyMutedUntilMs,
      relayAdminToken: relayAdminToken ?? this.relayAdminToken,
      useTor: useTor ?? this.useTor,
      proxyHost: proxyHost ?? this.proxyHost,
      proxyPort: proxyPort ?? this.proxyPort,
      proxyType: proxyType ?? this.proxyType,
    );
  }

  Map<String, dynamic> toJson() => {
        'themeMode': themeMode.name,
        'density': density.name,
        'accent': accent,
        'textScale': textScale,
        'localeTag': localeTag,
        'lastNotificationsSeenMs': lastNotificationsSeenMs,
        'lastChatSeenMs': lastChatSeenMs,
        'chatThreadSeenMs': chatThreadSeenMs,
        'pinnedChatThreads': pinnedChatThreads,
        'desktopUseColumns': desktopUseColumns,
        'mediaAutoplay': mediaAutoplay,
        'mediaMuted': mediaMuted,
        'emojiPickerScale': emojiPickerScale,
        'emojiPickerColumns': emojiPickerColumns,
        'emojiPickerStyle': emojiPickerStyle.name,
        'gifApiKey': gifApiKey,
        'translationProvider': translationProvider.name,
        'translationAuthKey': translationAuthKey,
        'translationUsePro': translationUsePro,
        'translationTimeoutMs': translationTimeoutMs,
        'translationDeepLxUrl': translationDeepLxUrl,
        'telemetryEnabled': telemetryEnabled,
        'clientMonitoringEnabled': clientMonitoringEnabled,
        'notifyChat': notifyChat,
        'notifyDirect': notifyDirect,
        'notifyMutedUntilMs': notifyMutedUntilMs,
        'relayAdminToken': relayAdminToken,
        'useTor': useTor,
        'proxyHost': proxyHost,
        'proxyPort': proxyPort,
        'proxyType': proxyType,
      };

  Map<String, dynamic> get json => toJson();

  static UiPrefs fromJson(Map<String, dynamic> raw) {
    UiThemeMode parseTheme(String? v) {
      for (final m in UiThemeMode.values) {
        if (m.name == v) return m;
      }
      return UiThemeMode.system;
    }

    UiDensity parseDensity(String? v) {
      for (final d in UiDensity.values) {
        if (d.name == v) return d;
      }
      return UiDensity.normal;
    }

    EmojiPickerStyle parseEmojiPickerStyle(String? v) {
      for (final s in EmojiPickerStyle.values) {
        if (s.name == v) return s;
      }
      return UiPrefs.defaults().emojiPickerStyle;
    }

    TranslationProvider parseTranslationProvider(String? v) {
      for (final p in TranslationProvider.values) {
        if (p.name == v) return p;
      }
      return UiPrefs.defaults().translationProvider;
    }

    final accent = (raw['accent'] is num) ? (raw['accent'] as num).toInt() : UiPrefs.defaults().accent;
    final scale = (raw['textScale'] is num) ? (raw['textScale'] as num).toDouble() : UiPrefs.defaults().textScale;
    final timeoutMs = (raw['translationTimeoutMs'] is num) ? (raw['translationTimeoutMs'] as num).toInt() : UiPrefs.defaults().translationTimeoutMs;
    final mutedUntilMs = (raw['notifyMutedUntilMs'] is num) ? (raw['notifyMutedUntilMs'] as num).toInt() : UiPrefs.defaults().notifyMutedUntilMs;
    final seenRaw = raw['chatThreadSeenMs'];
    final seenMap = <String, int>{};
    if (seenRaw is Map) {
      for (final entry in seenRaw.entries) {
        final key = entry.key.toString();
        final value = entry.value;
        if (key.isEmpty) continue;
        if (value is num) {
          seenMap[key] = value.toInt();
        } else if (value is String) {
          final parsed = int.tryParse(value);
          if (parsed != null) seenMap[key] = parsed;
        }
      }
    }
    final rawGifKey = (raw['gifApiKey'] as String? ?? '').trim();
    final gifApiKey = (rawGifKey == _legacyTenorKey || rawGifKey.isEmpty) ? _defaultGiphyKey : rawGifKey;
    return UiPrefs(
      themeMode: parseTheme(raw['themeMode']?.toString()),
      density: parseDensity(raw['density']?.toString()),
      accent: accent,
      textScale: scale.clamp(0.75, 1.5),
      localeTag: (raw['localeTag'] as String? ?? '').trim(),
      lastNotificationsSeenMs: (raw['lastNotificationsSeenMs'] is num) ? (raw['lastNotificationsSeenMs'] as num).toInt() : 0,
      lastChatSeenMs: (raw['lastChatSeenMs'] is num) ? (raw['lastChatSeenMs'] as num).toInt() : 0,
      chatThreadSeenMs: seenMap,
      pinnedChatThreads: (raw['pinnedChatThreads'] is List)
          ? (raw['pinnedChatThreads'] as List).map((e) => e.toString()).where((e) => e.isNotEmpty).toList()
          : const [],
      desktopUseColumns: raw['desktopUseColumns'] == null ? UiPrefs.defaults().desktopUseColumns : raw['desktopUseColumns'] == true,
      mediaAutoplay: raw['mediaAutoplay'] == null ? UiPrefs.defaults().mediaAutoplay : raw['mediaAutoplay'] == true,
      mediaMuted: raw['mediaMuted'] == null ? UiPrefs.defaults().mediaMuted : raw['mediaMuted'] == true,
      emojiPickerScale: (raw['emojiPickerScale'] is num) ? (raw['emojiPickerScale'] as num).toDouble().clamp(0.7, 1.6) : UiPrefs.defaults().emojiPickerScale,
      emojiPickerColumns: (raw['emojiPickerColumns'] is num) ? (raw['emojiPickerColumns'] as num).toInt().clamp(5, 12) : UiPrefs.defaults().emojiPickerColumns,
      emojiPickerStyle: parseEmojiPickerStyle(raw['emojiPickerStyle']?.toString()),
      gifApiKey: gifApiKey,
      translationProvider: parseTranslationProvider(raw['translationProvider']?.toString()),
      translationAuthKey: (raw['translationAuthKey'] as String? ?? '').trim(),
      translationUsePro: raw['translationUsePro'] == true,
      translationTimeoutMs: timeoutMs.clamp(2000, 60000),
      translationDeepLxUrl: (raw['translationDeepLxUrl'] as String? ?? UiPrefs.defaults().translationDeepLxUrl).trim(),
      telemetryEnabled: raw['telemetryEnabled'] == true,
      clientMonitoringEnabled: raw['clientMonitoringEnabled'] == true,
      notifyChat: raw['notifyChat'] == null ? UiPrefs.defaults().notifyChat : raw['notifyChat'] == true,
      notifyDirect: raw['notifyDirect'] == null ? UiPrefs.defaults().notifyDirect : raw['notifyDirect'] == true,
      notifyMutedUntilMs: mutedUntilMs,
      relayAdminToken: (raw['relayAdminToken'] as String? ?? '').trim(),
      useTor: raw['useTor'] == true,
      proxyHost: (raw['proxyHost'] as String? ?? '').trim(),
      proxyPort: (raw['proxyPort'] is num) ? (raw['proxyPort'] as num).toInt() : null,
      proxyType: (raw['proxyType'] as String? ?? '').trim(),
    );
  }

  ThemeMode get flutterThemeMode {
    switch (themeMode) {
      case UiThemeMode.light:
        return ThemeMode.light;
      case UiThemeMode.dark:
        return ThemeMode.dark;
      case UiThemeMode.system:
        return ThemeMode.system;
    }
  }

  Locale? get flutterLocale {
    final tag = localeTag.trim();
    if (tag.isEmpty) return null;
    return Locale(tag);
  }

  VisualDensity get visualDensity => density == UiDensity.compact ? VisualDensity.compact : VisualDensity.standard;

  Color get accentColor => Color(accent);
}
