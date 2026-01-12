/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import 'ui_tokens.dart';

class MisskeyTheme {
  static ThemeData dark({Color? accent, VisualDensity? density}) {
    const bg = Color(0xFF0B0D11);
    const surface = Color(0xFF121622);
    const surface2 = Color(0xFF161B2A);
    final primary = accent ?? const Color(0xFF4AA8FF);
    const error = Color(0xFFFF5C5C);

    final scheme = const ColorScheme.dark(
      primary: Color(0xFF4AA8FF),
      secondary: Color(0xFF7B61FF),
      surface: surface,
      error: error,
      onPrimary: Colors.black,
      onSecondary: Colors.white,
      onSurface: Color(0xFFE7EAF0),
      onError: Colors.black,
    ).copyWith(primary: primary, surfaceContainerHighest: surface2);

    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      scaffoldBackgroundColor: bg,
      visualDensity: density ?? VisualDensity.standard,
      appBarTheme: const AppBarTheme(
        backgroundColor: bg,
        foregroundColor: Color(0xFFE7EAF0),
        elevation: 0,
      ),
      cardTheme: const CardThemeData(
        color: surface,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.all(Radius.circular(UiTokens.radiusCard))),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: surface2,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusInput),
          borderSide: BorderSide.none,
        ),
      ),
      dividerColor: const Color(0xFF27304A),
      navigationRailTheme: const NavigationRailThemeData(
        backgroundColor: bg,
      ),
    );
  }

  static ThemeData light({Color? accent, VisualDensity? density}) {
    const bg = Color(0xFFF6F7FB);
    const surface = Colors.white;
    const surface2 = Color(0xFFF0F2F8);
    final primary = accent ?? const Color(0xFF007AFF);
    const error = Color(0xFFCC2B2B);

    final scheme = const ColorScheme.light(
      primary: Color(0xFF007AFF),
      secondary: Color(0xFF7B61FF),
      surface: surface,
      error: error,
      onPrimary: Colors.white,
      onSecondary: Colors.white,
      onSurface: Color(0xFF10131A),
      onError: Colors.white,
    ).copyWith(primary: primary, surfaceContainerHighest: surface2);

    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      scaffoldBackgroundColor: bg,
      visualDensity: density ?? VisualDensity.standard,
      cardTheme: const CardThemeData(
        color: surface,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.all(Radius.circular(UiTokens.radiusCard))),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: surface2,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusInput),
          borderSide: BorderSide.none,
        ),
      ),
      navigationRailTheme: const NavigationRailThemeData(
        backgroundColor: bg,
      ),
    );
  }
}
