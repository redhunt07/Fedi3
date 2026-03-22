/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../services/rss_feed_service.dart';
import '../utils/open_url.dart';

class RssTicker extends StatefulWidget {
  const RssTicker({super.key});

  @override
  State<RssTicker> createState() => _RssTickerState();
}

class _RssTickerState extends State<RssTicker> {
  final ScrollController _scroll = ScrollController();
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(milliseconds: 90), (_) => _tick());
  }

  @override
  void dispose() {
    _timer?.cancel();
    _scroll.dispose();
    super.dispose();
  }

  void _tick() {
    if (!_scroll.hasClients) return;
    final max = _scroll.position.maxScrollExtent;
    if (max <= 0) return;
    final next = _scroll.offset + 1.4;
    if (next >= max) {
      _scroll.jumpTo(0);
      return;
    }
    _scroll.jumpTo(next);
  }

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<List<RssItem>>(
      valueListenable: RssFeedService.instance.items,
      builder: (context, items, _) {
        if (items.isEmpty) return const SizedBox.shrink();
        return Container(
          height: 38,
          margin: const EdgeInsets.fromLTRB(12, 8, 12, 0),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(12),
            border: Border.all(
              color: Theme.of(context).colorScheme.outlineVariant.withAlpha(90),
            ),
            color: Theme.of(context).colorScheme.surfaceContainerLow,
          ),
          child: ListView(
            controller: _scroll,
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
            children: [
              for (final item in items.take(40))
                Padding(
                  padding: const EdgeInsets.only(right: 12),
                  child: InkWell(
                    borderRadius: BorderRadius.circular(8),
                    onTap: () => openUrlExternal(item.link),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 6, vertical: 3),
                      child: Text(
                        item.title,
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: Theme.of(context).colorScheme.primary,
                        ),
                      ),
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
