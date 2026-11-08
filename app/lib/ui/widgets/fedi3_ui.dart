/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../theme/ui_tokens.dart';

/// Brand mark for the app shell: a fixed aurora gradient tile with the Fedi3
/// glyph and a slow, subtle particle field. The tile itself never moves; only
/// the particles drift, which keeps the mark calm and recognizable.
class BrandMark extends StatefulWidget {
  const BrandMark({
    super.key,
    this.size = 44,
    this.showGlow = true,
    this.animated = false,
  });

  final double size;
  final bool showGlow;
  final bool animated;

  @override
  State<BrandMark> createState() => _BrandMarkState();
}

class _BrandMarkState extends State<BrandMark>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 14),
  );

  @override
  void initState() {
    super.initState();
    if (widget.animated) _c.repeat(count: 2);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reduce = MediaQuery.of(context).disableAnimations;
    if (!widget.animated || reduce) {
      if (_c.isAnimating) _c.stop();
    } else if (!_c.isAnimating && _c.value == 0) {
      _c.repeat(count: 2);
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final size = widget.size;
    final radius = BorderRadius.circular(size * 0.32);
    return Semantics(
      label: 'Fedi3',
      button: true,
      child: Container(
        width: size,
        height: size,
        decoration: BoxDecoration(
          borderRadius: radius,
          gradient: const LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [
              Color(UiTokens.brandViolet),
              Color(0xFF5B8DF6),
              Color(UiTokens.brandCyan),
            ],
            stops: [0, 0.55, 1],
          ),
          border: Border.all(color: Colors.white.withAlpha(36)),
          boxShadow: widget.showGlow
              ? [
                  BoxShadow(
                    color: const Color(UiTokens.brandViolet).withAlpha(90),
                    blurRadius: size * 0.75,
                    offset: const Offset(0, 6),
                  ),
                  BoxShadow(
                    color: const Color(UiTokens.brandCyan).withAlpha(45),
                    blurRadius: size * 0.45,
                    offset: const Offset(0, 2),
                  ),
                ]
              : null,
        ),
        child: ClipRRect(
          borderRadius: radius,
          child: Stack(
            children: [
              Positioned.fill(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    gradient: RadialGradient(
                      center: const Alignment(-0.35, -0.45),
                      radius: 0.9,
                      colors: [
                        Colors.white.withAlpha(46),
                        Colors.white.withAlpha(0),
                      ],
                    ),
                  ),
                ),
              ),
              if (widget.animated)
                Positioned.fill(
                  child: IgnorePointer(
                    child: AnimatedBuilder(
                      animation: _c,
                      builder: (context, _) => CustomPaint(
                        painter: _BrandParticlesPainter(progress: _c.value),
                      ),
                    ),
                  ),
                ),
              Center(
                child: Icon(
                  Icons.hub_rounded,
                  color: Colors.white,
                  size: size * 0.52,
                  shadows: [
                    Shadow(
                      color: Colors.black.withAlpha(70),
                      blurRadius: size * 0.08,
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Small drifting sparks inside the brand tile. Deterministic per index so the
/// mark stays cheap to repaint and does not flicker.
class _BrandParticlesPainter extends CustomPainter {
  const _BrandParticlesPainter({required this.progress});

  final double progress;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..style = PaintingStyle.fill;
    for (var i = 0; i < 11; i++) {
      final phase = (i * 0.173 + progress) % 1.0;
      final x = size.width * (0.08 + 0.84 * ((i * 0.37) % 1.0));
      final drift = math.sin((phase + i) * 2 * math.pi) * size.width * 0.06;
      final y = size.height * (0.12 + 0.76 * phase);
      final twinkle = 0.5 + 0.5 * math.sin((progress + i * 0.31) * 2 * math.pi);
      paint.color = Colors.white.withAlpha((28 + 52 * twinkle).round());
      canvas.drawCircle(
          Offset(x + drift, y), size.width * (0.008 + 0.006 * twinkle), paint);
    }
  }

  @override
  bool shouldRepaint(covariant _BrandParticlesPainter oldDelegate) =>
      oldDelegate.progress != progress;
}

/// Primary call to action with the aurora gradient and a subtle hover lift.
class AuroraButton extends StatefulWidget {
  const AuroraButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.icon,
    this.expand = false,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final bool expand;

  @override
  State<AuroraButton> createState() => _AuroraButtonState();
}

class _AuroraButtonState extends State<AuroraButton> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onPressed != null;
    final child = Row(
      mainAxisSize: widget.expand ? MainAxisSize.max : MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        if (widget.icon != null) ...[
          Icon(widget.icon, size: 18, color: Colors.white),
          const SizedBox(width: 8),
        ],
        Text(
          widget.label,
          style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: Colors.white,
                fontWeight: FontWeight.w800,
              ),
        ),
      ],
    );
    return MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: AnimatedScale(
        scale: _hover && enabled ? 1.02 : 1,
        duration: UiTokens.motionFast,
        child: AnimatedOpacity(
          opacity: enabled ? 1 : 0.5,
          duration: UiTokens.motionFast,
          child: DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(UiTokens.radiusPill),
              gradient: const LinearGradient(
                colors: [
                  Color(UiTokens.brandViolet),
                  Color(UiTokens.brandCyan),
                ],
              ),
              boxShadow: _hover && enabled
                  ? [
                      BoxShadow(
                        color: const Color(UiTokens.brandViolet).withAlpha(120),
                        blurRadius: 22,
                        offset: const Offset(0, 8),
                      ),
                    ]
                  : null,
            ),
            child: Material(
              color: Colors.transparent,
              child: InkWell(
                borderRadius: BorderRadius.circular(UiTokens.radiusPill),
                onTap: widget.onPressed,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                      horizontal: UiTokens.gapLg, vertical: 13),
                  child: child,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// A soft, bordered surface used for panels and cards in the new visual language.
class GlassPanel extends StatelessWidget {
  const GlassPanel({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(UiTokens.padCard),
    this.onTap,
    this.onLongPress,
    this.highlight = false,
  });

  final Widget child;
  final EdgeInsetsGeometry padding;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;
  final bool highlight;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final base = isDark
        ? const Color(UiTokens.darkSurface)
        : const Color(UiTokens.lightSurface);
    final border = isDark
        ? const Color(UiTokens.darkBorder)
        : const Color(UiTokens.lightBorder);
    final content = Container(
      padding: padding,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(UiTokens.radiusCard),
        color: highlight ? scheme.primary.withAlpha(isDark ? 18 : 12) : base,
        border: Border.all(
          color: highlight ? scheme.primary.withAlpha(90) : border,
        ),
        boxShadow: isDark
            ? [
                BoxShadow(
                  color: Colors.black.withAlpha(90),
                  blurRadius: 26,
                  offset: const Offset(0, 12),
                ),
              ]
            : [
                BoxShadow(
                  color: scheme.shadow.withAlpha(10),
                  blurRadius: 20,
                  offset: const Offset(0, 8),
                ),
              ],
      ),
      child: child,
    );
    if (onTap == null && onLongPress == null) return content;
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: BorderRadius.circular(UiTokens.radiusCard),
        onTap: onTap,
        onLongPress: onLongPress,
        child: content,
      ),
    );
  }
}

/// Small pill label used for tags, counters and status hints.
class PillTag extends StatelessWidget {
  const PillTag({
    super.key,
    required this.label,
    this.icon,
    this.color,
  });

  final String label;
  final IconData? icon;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final tint = color ?? scheme.onSurfaceVariant;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
      decoration: BoxDecoration(
        color: tint.withAlpha(18),
        borderRadius: BorderRadius.circular(UiTokens.radiusPill),
        border: Border.all(color: tint.withAlpha(60)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 13, color: tint),
            const SizedBox(width: 5),
          ],
          Text(
            label,
            style: Theme.of(context).textTheme.labelSmall?.copyWith(
                  color: tint,
                  fontWeight: FontWeight.w700,
                ),
          ),
        ],
      ),
    );
  }
}

/// Live status dot with a soft pulse, used for core/relay connectivity.
class StatusDot extends StatefulWidget {
  const StatusDot({super.key, required this.color, this.size = 8});

  final Color color;
  final double size;

  @override
  State<StatusDot> createState() => _StatusDotState();
}

class _StatusDotState extends State<StatusDot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1800),
  );

  @override
  void initState() {
    super.initState();
    // Pulse a few times when the dot appears, then settle. A never-ending glow
    // keeps every avatar repainting and drains battery on a static screen.
    _controller.repeat(reverse: true, count: 4);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _controller,
      builder: (_, __) {
        final t = _controller.value;
        return Container(
          width: widget.size,
          height: widget.size,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: widget.color,
            boxShadow: [
              BoxShadow(
                color: widget.color.withAlpha((60 + 90 * t).round()),
                blurRadius: widget.size * 2.2,
                spreadRadius: 1,
              ),
            ],
          ),
        );
      },
    );
  }
}

/// Tooltip wrapper that also exposes a matching semantics label.
///
/// Icon-only controls often communicate their purpose only through a glyph;
/// this widget makes the tooltip text the accessible name as well, and lets
/// callers expose a keyboard shortcut hint (for example "Search (Ctrl+2)").
class AccessibleTooltip extends StatelessWidget {
  const AccessibleTooltip({
    super.key,
    required this.message,
    required this.child,
    this.button = false,
    this.selected,
    this.excludeFromSemantics = true,
  });

  final String message;
  final Widget child;
  final bool button;
  final bool? selected;

  /// When true the child's own semantics are replaced by [message]. This keeps
  /// the accessible name identical to the visible tooltip and avoids duplicate
  /// announcements for buttons that also render a text label.
  final bool excludeFromSemantics;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      label: message,
      button: button,
      selected: selected,
      excludeSemantics: excludeFromSemantics,
      child: Tooltip(
        message: message,
        waitDuration: const Duration(milliseconds: 450),
        excludeFromSemantics: true,
        child: child,
      ),
    );
  }
}

class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    this.message,
    this.action,
  });

  final IconData icon;
  final String title;
  final String? message;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: 72,
              height: 72,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                gradient: LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: [
                    scheme.primary.withAlpha(60),
                    scheme.secondary.withAlpha(40),
                  ],
                ),
                border: Border.all(color: scheme.primary.withAlpha(90)),
              ),
              child: Icon(icon, size: 32, color: scheme.primary),
            ),
            const SizedBox(height: 18),
            Text(
              title,
              textAlign: TextAlign.center,
              style: Theme.of(context)
                  .textTheme
                  .titleMedium
                  ?.copyWith(fontWeight: FontWeight.w800),
            ),
            if (message != null) ...[
              const SizedBox(height: 8),
              Text(
                message!,
                textAlign: TextAlign.center,
                style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      color: scheme.onSurfaceVariant,
                    ),
              ),
            ],
            if (action != null) ...[
              const SizedBox(height: 18),
              action!,
            ],
          ],
        ),
      ),
    );
  }
}
