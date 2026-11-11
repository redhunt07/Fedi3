/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:material_ui/material_ui.dart';

import 'ui_tokens.dart';

/// Fedi3 "Aurora" theme.
///
/// The class name is kept for compatibility with existing imports; the visual
/// language lives here and in [UiTokens].
class MisskeyTheme {
  static ThemeData dark({Color? accent, VisualDensity? density}) {
    final primary = accent ?? const Color(UiTokens.brandViolet);
    final scheme = ColorScheme.dark(
      primary: primary,
      secondary: const Color(UiTokens.brandCyan),
      surface: const Color(UiTokens.darkSurface),
      surfaceContainerLow: const Color(UiTokens.darkBg),
      surfaceContainer: const Color(UiTokens.darkSurface),
      surfaceContainerHigh: const Color(UiTokens.darkSurfaceAlt),
      surfaceContainerHighest: const Color(UiTokens.darkSurfaceAlt),
      onPrimary: Colors.white,
      onSecondary: const Color(0xFF04121A),
      onSurface: const Color(UiTokens.darkText),
      onSurfaceVariant: const Color(UiTokens.darkMuted),
      outline: const Color(0xFF2A3040),
      outlineVariant: const Color(UiTokens.darkBorder),
      error: const Color(0xFFFF6B81),
      onError: const Color(0xFF1A0509),
      shadow: Colors.black,
    );
    return _build(
      scheme: scheme,
      background: const Color(UiTokens.darkBg),
      card: const Color(UiTokens.darkSurface),
      density: density,
      brightness: Brightness.dark,
    );
  }

  static ThemeData light({Color? accent, VisualDensity? density}) {
    final primary = accent ?? const Color(UiTokens.brandViolet);
    final scheme = ColorScheme.light(
      primary: primary,
      secondary: const Color(0xFF0891B2),
      surface: const Color(UiTokens.lightSurface),
      surfaceContainerLow: const Color(UiTokens.lightBg),
      surfaceContainer: const Color(UiTokens.lightSurface),
      surfaceContainerHigh: const Color(UiTokens.lightSurfaceAlt),
      surfaceContainerHighest: const Color(UiTokens.lightSurfaceAlt),
      onPrimary: Colors.white,
      onSecondary: Colors.white,
      onSurface: const Color(UiTokens.lightText),
      onSurfaceVariant: const Color(UiTokens.lightMuted),
      outline: const Color(0xFFD5D9E4),
      outlineVariant: const Color(UiTokens.lightBorder),
      error: const Color(0xFFD92D4B),
      onError: Colors.white,
      shadow: const Color(0xFF0B0D12),
    );
    return _build(
      scheme: scheme,
      background: const Color(UiTokens.lightBg),
      card: const Color(UiTokens.lightSurface),
      density: density,
      brightness: Brightness.light,
    );
  }

  static ThemeData _build({
    required ColorScheme scheme,
    required Color background,
    required Color card,
    required Brightness brightness,
    VisualDensity? density,
  }) {
    final base = ThemeData(
      useMaterial3: true,
      brightness: brightness,
      colorScheme: scheme,
      visualDensity: density ?? VisualDensity.standard,
    );
    final text = _textTheme(base.textTheme, scheme);
    final isDark = brightness == Brightness.dark;
    final border = isDark
        ? const Color(UiTokens.darkBorder)
        : const Color(UiTokens.lightBorder);

    return base.copyWith(
      scaffoldBackgroundColor: background,
      textTheme: text,
      primaryTextTheme: text,
      appBarTheme: AppBarTheme(
        backgroundColor: background,
        foregroundColor: scheme.onSurface,
        elevation: 0,
        scrolledUnderElevation: 0,
        centerTitle: false,
        titleTextStyle: text.titleLarge?.copyWith(
          fontWeight: FontWeight.w800,
          letterSpacing: -0.4,
        ),
      ),
      cardTheme: CardThemeData(
        color: card,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        margin: EdgeInsets.zero,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusCard),
          side: BorderSide(color: border, width: 1),
        ),
      ),
      dividerTheme: DividerThemeData(
        color: border,
        thickness: 1,
        space: 1,
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: scheme.surfaceContainerHighest,
        hintStyle: text.bodyMedium?.copyWith(color: scheme.onSurfaceVariant),
        contentPadding: const EdgeInsets.symmetric(
            horizontal: UiTokens.padCard, vertical: 14),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusInput),
          borderSide: BorderSide(color: border),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusInput),
          borderSide: BorderSide(color: border),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusInput),
          borderSide: BorderSide(color: scheme.primary, width: 1.4),
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          backgroundColor: scheme.primary,
          foregroundColor: scheme.onPrimary,
          minimumSize: const Size(0, 44),
          padding: const EdgeInsets.symmetric(
              horizontal: UiTokens.gapLg, vertical: 12),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(UiTokens.radiusPill),
          ),
          textStyle: text.labelLarge?.copyWith(fontWeight: FontWeight.w700),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          foregroundColor: scheme.onSurface,
          minimumSize: const Size(0, 44),
          side: BorderSide(color: border),
          padding: const EdgeInsets.symmetric(
              horizontal: UiTokens.gapLg, vertical: 12),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(UiTokens.radiusPill),
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          foregroundColor: scheme.primary,
          textStyle: text.labelLarge?.copyWith(fontWeight: FontWeight.w700),
        ),
      ),
      chipTheme: ChipThemeData(
        backgroundColor: scheme.surfaceContainerHighest,
        side: BorderSide(color: border),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusPill),
        ),
        labelStyle: text.labelMedium,
      ),
      navigationRailTheme: NavigationRailThemeData(
        backgroundColor: background,
        indicatorColor: scheme.primary.withAlpha(28),
        selectedIconTheme: IconThemeData(color: scheme.primary),
        unselectedIconTheme: IconThemeData(color: scheme.onSurfaceVariant),
        selectedLabelTextStyle: text.labelMedium?.copyWith(
          color: scheme.onSurface,
          fontWeight: FontWeight.w700,
        ),
        unselectedLabelTextStyle:
            text.labelMedium?.copyWith(color: scheme.onSurfaceVariant),
      ),
      navigationBarTheme: NavigationBarThemeData(
        backgroundColor: background,
        indicatorColor: scheme.primary.withAlpha(28),
        iconTheme: WidgetStateProperty.resolveWith((states) => IconThemeData(
              color: states.contains(WidgetState.selected)
                  ? scheme.primary
                  : scheme.onSurfaceVariant,
            )),
        labelTextStyle: WidgetStatePropertyAll(text.labelSmall),
      ),
      snackBarTheme: SnackBarThemeData(
        backgroundColor: scheme.surfaceContainerHigh,
        contentTextStyle: text.bodyMedium,
        behavior: SnackBarBehavior.floating,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusCardSm),
        ),
      ),
      tooltipTheme: TooltipThemeData(
        decoration: BoxDecoration(
          color: scheme.surfaceContainerHigh,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: border),
        ),
        textStyle: text.bodySmall,
      ),
      iconButtonTheme: IconButtonThemeData(
        style: IconButton.styleFrom(
          foregroundColor: scheme.onSurfaceVariant,
          highlightColor: scheme.primary.withAlpha(20),
        ),
      ),
      progressIndicatorTheme: ProgressIndicatorThemeData(color: scheme.primary),
      listTileTheme: ListTileThemeData(
        iconColor: scheme.onSurfaceVariant,
        textColor: scheme.onSurface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(UiTokens.radiusCardSm),
        ),
      ),
    );
  }

  static TextTheme _textTheme(TextTheme base, ColorScheme scheme) {
    TextStyle t(TextStyle? s,
        {double? size, FontWeight? weight, double? spacing, double? height}) {
      return (s ?? const TextStyle()).copyWith(
        color: scheme.onSurface,
        fontSize: size,
        fontWeight: weight,
        letterSpacing: spacing,
        height: height,
      );
    }

    return base.copyWith(
      displaySmall: t(base.displaySmall,
          size: 30, weight: FontWeight.w800, spacing: -0.8, height: 1.15),
      headlineMedium: t(base.headlineMedium,
          size: 24, weight: FontWeight.w800, spacing: -0.5, height: 1.2),
      headlineSmall: t(base.headlineSmall,
          size: 20, weight: FontWeight.w700, spacing: -0.3),
      titleLarge:
          t(base.titleLarge, size: 18, weight: FontWeight.w700, spacing: -0.2),
      titleMedium: t(base.titleMedium, size: 15, weight: FontWeight.w700),
      titleSmall: t(base.titleSmall, size: 13, weight: FontWeight.w700),
      bodyLarge: t(base.bodyLarge, size: 15, height: 1.45),
      bodyMedium: t(base.bodyMedium, size: 14, height: 1.45),
      bodySmall: t(base.bodySmall, size: 12.5, height: 1.4),
      labelLarge: t(base.labelLarge, size: 13, weight: FontWeight.w700),
      labelMedium: t(base.labelMedium, size: 12, weight: FontWeight.w600),
      labelSmall: t(base.labelSmall, size: 11, weight: FontWeight.w600),
    );
  }
}
