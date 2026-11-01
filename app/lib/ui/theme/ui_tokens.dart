/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

/// Design tokens for the Fedi3 client.
///
/// The visual language is "Aurora": deep neutral surfaces, a violet/cyan accent
/// pair, soft borders and generous radii. Keep values here instead of hardcoding
/// colors in widgets so the theme can evolve in one place.
class UiTokens {
  static const double desktopBreakpoint = 1000;
  static const double contentMaxWidth = 720;
  static const double rightSidebarWidth = 312;
  static const double timelineColumnWidth = 420;
  static const double railWidth = 100;
  static const double railWidthExpanded = 212;

  static const double radiusCard = 18;
  static const double radiusCardSm = 14;
  static const double radiusInput = 14;
  static const double radiusPill = 999;

  static const double padScreen = 20;
  static const double padCard = 16;
  static const double gapXs = 4;
  static const double gapSm = 8;
  static const double gapMd = 12;
  static const double gapLg = 20;
  static const double gapXl = 28;

  static const Duration motionFast = Duration(milliseconds: 140);
  static const Duration motionBase = Duration(milliseconds: 220);
  static const Duration motionSlow = Duration(milliseconds: 420);

  // Brand accents. A user-selected accent overrides the primary color only.
  static const int brandViolet = 0xFF7C6CF6;
  static const int brandCyan = 0xFF22D3EE;
  static const int brandPink = 0xFFF471B5;

  // Dark surfaces.
  static const int darkBg = 0xFF07080D;
  static const int darkSurface = 0xFF10131C;
  static const int darkSurfaceAlt = 0xFF161A26;
  static const int darkBorder = 0x14FFFFFF;
  static const int darkText = 0xFFECEDF3;
  static const int darkMuted = 0xFF9AA1B2;

  // Light surfaces.
  static const int lightBg = 0xFFF5F6FB;
  static const int lightSurface = 0xFFFFFFFF;
  static const int lightSurfaceAlt = 0xFFEEF0F7;
  static const int lightBorder = 0x140B0D12;
  static const int lightText = 0xFF0D1017;
  static const int lightMuted = 0xFF5C6474;

  /// Gradient used for primary calls to action and the brand mark.
  static const List<int> auroraGradient = [brandViolet, brandCyan];
}
