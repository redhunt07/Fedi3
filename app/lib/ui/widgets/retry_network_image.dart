/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

class RetryNetworkImage extends StatefulWidget {
  const RetryNetworkImage({
    super.key,
    required this.url,
    required this.width,
    required this.height,
    this.fit = BoxFit.cover,
    this.cacheWidth,
    this.cacheHeight,
    this.borderRadius,
    this.placeholder,
    this.errorIcon = Icons.refresh,
  });

  final String url;
  final double width;
  final double height;
  final BoxFit fit;
  final int? cacheWidth;
  final int? cacheHeight;
  final BorderRadius? borderRadius;
  final Widget? placeholder;
  final IconData errorIcon;

  @override
  State<RetryNetworkImage> createState() => _RetryNetworkImageState();
}

class _RetryNetworkImageState extends State<RetryNetworkImage> {
  int _retry = 0;

  @override
  Widget build(BuildContext context) {
    final u = widget.url.trim();
    if (u.isEmpty) {
      return _buildPlaceholder(context, isError: false);
    }
    final url = _appendRetry(u, _retry);
    final img = Image.network(
      url,
      width: widget.width,
      height: widget.height,
      fit: widget.fit,
      cacheWidth: widget.cacheWidth,
      cacheHeight: widget.cacheHeight,
      filterQuality: FilterQuality.low,
      errorBuilder: (_, __, ___) => _buildPlaceholder(context, isError: true),
      loadingBuilder: (context, child, progress) {
        if (progress == null) return child;
        return _buildPlaceholder(context, isError: false);
      },
    );
    if (widget.borderRadius == null) return img;
    return ClipRRect(borderRadius: widget.borderRadius, child: img);
  }

  Widget _buildPlaceholder(BuildContext context, {required bool isError}) {
    if (widget.placeholder != null) return widget.placeholder!;
    final bg = Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(140);
    return InkWell(
      onTap: isError ? () => setState(() => _retry += 1) : null,
      child: Container(
        width: widget.width,
        height: widget.height,
        alignment: Alignment.center,
        color: bg,
        child: isError ? Icon(widget.errorIcon, size: 18) : const SizedBox.shrink(),
      ),
    );
  }

  String _appendRetry(String url, int retry) {
    if (retry == 0) return url;
    final join = url.contains('?') ? '&' : '?';
    return '$url${join}retry=$retry';
  }
}
