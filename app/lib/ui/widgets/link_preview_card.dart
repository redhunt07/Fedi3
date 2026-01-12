/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/link_preview_repository.dart';
import '../utils/open_url.dart';

class LinkPreviewCard extends StatelessWidget {
  const LinkPreviewCard({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final u = url.trim();
    if (u.isEmpty) return const SizedBox.shrink();
    final host = Uri.tryParse(u)?.host ?? '';

    return FutureBuilder<LinkPreview?>(
      future: LinkPreviewRepository.instance.get(u),
      builder: (context, snap) {
        final p = snap.data;
        if (snap.connectionState == ConnectionState.waiting && p == null) {
          return _skeleton(context, host: host, url: u);
        }
        if (p == null) return _fallback(host: host, url: u);

        final title = p.title.isNotEmpty ? p.title : host;
        final desc = p.description;
        final img = p.imageUrl.trim();
        final openTarget = u;

        return Card(
          child: InkWell(
            borderRadius: BorderRadius.circular(14),
            onTap: () => openUrlExternal(openTarget),
            child: Row(
              children: [
                if (img.isNotEmpty)
                  ClipRRect(
                    borderRadius: const BorderRadius.horizontal(left: Radius.circular(14)),
                    child: Image.network(
                      img,
                      width: 96,
                      height: 96,
                      fit: BoxFit.cover,
                      errorBuilder: (_, __, ___) => const SizedBox(width: 96, height: 96),
                    ),
                  ),
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(title, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w800)),
                        if (desc.isNotEmpty) ...[
                          const SizedBox(height: 6),
                          Text(
                            desc,
                            maxLines: 3,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12),
                          ),
                        ],
                        const SizedBox(height: 8),
                        Text(
                          host.isNotEmpty ? host : u,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
                        ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Padding(
                  padding: const EdgeInsets.only(right: 12),
                  child: _CopyLinkButton(url: u),
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _fallback({required String host, required String url}) {
    return Card(
      child: ListTile(
        leading: const Icon(Icons.link),
        title: Text(host.isNotEmpty ? host : url, maxLines: 1, overflow: TextOverflow.ellipsis),
        subtitle: Text(url, maxLines: 1, overflow: TextOverflow.ellipsis),
        trailing: _CopyLinkButton(url: url),
        onTap: () => openUrlExternal(url),
      ),
    );
  }

  Widget _skeleton(BuildContext context, {required String host, required String url}) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          children: [
            Container(
              width: 96,
              height: 96,
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120),
                borderRadius: BorderRadius.circular(12),
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(height: 14, width: double.infinity, color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120)),
                  const SizedBox(height: 8),
                  Container(height: 12, width: double.infinity, color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(100)),
                  const SizedBox(height: 8),
                  Container(height: 12, width: 160, color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(80)),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _CopyLinkButton extends StatelessWidget {
  const _CopyLinkButton({required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    return IconButton(
      tooltip: context.l10n.copy,
      icon: const Icon(Icons.content_copy, size: 18),
      onPressed: () async {
        await Clipboard.setData(ClipboardData(text: url));
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.copied)));
      },
    );
  }
}
