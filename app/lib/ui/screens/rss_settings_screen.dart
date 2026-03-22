/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../services/rss_feed_service.dart';

class RssSettingsScreen extends StatefulWidget {
  const RssSettingsScreen({super.key});

  @override
  State<RssSettingsScreen> createState() => _RssSettingsScreenState();
}

class _RssSettingsScreenState extends State<RssSettingsScreen> {
  final TextEditingController _urlCtrl = TextEditingController();

  @override
  void dispose() {
    _urlCtrl.dispose();
    super.dispose();
  }

  Future<void> _addUrl() async {
    final value = _urlCtrl.text.trim();
    if (!(value.startsWith('http://') || value.startsWith('https://'))) return;
    final existing = List<String>.from(RssFeedService.instance.urls.value);
    if (!existing.contains(value)) {
      existing.add(value);
      await RssFeedService.instance.saveUrls(existing);
    }
    _urlCtrl.clear();
    if (mounted) setState(() {});
  }

  Future<void> _removeUrl(String url) async {
    final existing = List<String>.from(RssFeedService.instance.urls.value)
      ..remove(url);
    await RssFeedService.instance.saveUrls(existing);
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('RSS Feed')),
      body: ValueListenableBuilder<List<String>>(
        valueListenable: RssFeedService.instance.urls,
        builder: (context, urls, _) {
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              const Text(
                'Aggiungi feed RSS/Atom. I titoli scorrono in alto e si aggiornano ogni 10 minuti.',
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _urlCtrl,
                      decoration: const InputDecoration(
                        labelText: 'URL feed',
                        hintText: 'https://example.com/rss.xml',
                        border: OutlineInputBorder(),
                      ),
                      onSubmitted: (_) => _addUrl(),
                    ),
                  ),
                  const SizedBox(width: 8),
                  FilledButton(
                    onPressed: _addUrl,
                    child: const Text('Aggiungi'),
                  ),
                ],
              ),
              const SizedBox(height: 16),
              OutlinedButton.icon(
                onPressed: () => RssFeedService.instance.refreshNow(),
                icon: const Icon(Icons.refresh),
                label: const Text('Aggiorna adesso'),
              ),
              const SizedBox(height: 16),
              if (urls.isEmpty)
                const Text('Nessun feed configurato.')
              else
                ...urls.map(
                  (url) => Card(
                    child: ListTile(
                      title: Text(url,
                          maxLines: 1, overflow: TextOverflow.ellipsis),
                      trailing: IconButton(
                        icon: const Icon(Icons.delete_outline),
                        onPressed: () => _removeUrl(url),
                      ),
                    ),
                  ),
                ),
            ],
          );
        },
      ),
    );
  }
}
