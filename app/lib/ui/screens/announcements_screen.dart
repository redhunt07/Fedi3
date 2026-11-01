/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/announcements_service.dart';
import '../../state/announcements_store.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/markdown_view.dart';

class AnnouncementsScreen extends StatefulWidget {
  const AnnouncementsScreen({super.key});

  @override
  State<AnnouncementsScreen> createState() => _AnnouncementsScreenState();
}

class _AnnouncementsScreenState extends State<AnnouncementsScreen> {
  List<AnnouncementEntry> _entries = const [];
  bool _loading = true;
  String? _error;
  DateTime? _lastSync;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load({bool refresh = false}) async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final entries =
          await AnnouncementsService.load(refresh: refresh);
      final newest = entries.isEmpty ? null : entries.first.id;
      await AnnouncementsStore.markSeen(newest);
      if (!mounted) return;
      setState(() {
        _entries = entries;
        _lastSync = DateTime.now();
      });
    } catch (e) {
      final cached = await AnnouncementsStore.cached();
      final entries =
          cached == null ? const <AnnouncementEntry>[] : AnnouncementsService.parse(cached);
      if (!mounted) return;
      setState(() {
        _entries = entries;
        _error = context.l10n.announcementsOffline;
      });
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.announcementsTitle),
        actions: [
          IconButton(
            tooltip: context.l10n.announcementsRefresh,
            onPressed: _loading ? null : () => _load(refresh: true),
            icon: const Icon(Icons.refresh_rounded),
          ),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: () => _load(refresh: true),
        child: ListView(
          padding: const EdgeInsets.fromLTRB(20, 16, 20, 24),
          children: [
            GlassPanel(
              child: Row(
                children: [
                  Container(
                    width: 52,
                    height: 52,
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(16),
                      gradient: const LinearGradient(
                        colors: [
                          Color(UiTokens.brandViolet),
                          Color(UiTokens.brandCyan),
                        ],
                      ),
                    ),
                    child: const Icon(Icons.campaign_rounded,
                        color: Colors.white, size: 26),
                  ),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          context.l10n.announcementsSubtitle,
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                        const SizedBox(height: 4),
                        Text(
                          _lastSync == null
                              ? context.l10n.announcementsSource
                              : context.l10n.announcementsLastSync(
                                  '${_lastSync!.hour.toString().padLeft(2, '0')}:${_lastSync!.minute.toString().padLeft(2, '0')}'),
                          style: Theme.of(context)
                              .textTheme
                              .labelSmall
                              ?.copyWith(
                                color: Theme.of(context)
                                    .colorScheme
                                    .onSurfaceVariant,
                              ),
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    tooltip: context.l10n.announcementsSource,
                    onPressed: () => launchUrl(
                      Uri.parse(
                          'https://github.com/redhunt07/Fedi3/blob/main/ANNOUNCEMENTS.md'),
                      mode: LaunchMode.externalApplication,
                    ),
                    icon: const Icon(Icons.open_in_new_rounded, size: 18),
                  ),
                ],
              ),
            ),
            if (_error != null) ...[
              const SizedBox(height: 10),
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Text(_error!),
                ),
              ),
            ],
            const SizedBox(height: 12),
            if (_loading)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 40),
                child: Center(child: CircularProgressIndicator()),
              )
            else if (_entries.isEmpty)
              EmptyState(
                icon: Icons.campaign_outlined,
                title: context.l10n.announcementsEmpty,
              )
            else
              for (final entry in _entries) ...[
                _AnnouncementCard(entry: entry),
                const SizedBox(height: 10),
              ],
          ],
        ),
      ),
    );
  }
}

class _AnnouncementCard extends StatelessWidget {
  const _AnnouncementCard({required this.entry});

  final AnnouncementEntry entry;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(UiTokens.padCard),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(99),
                    color: scheme.primary.withAlpha(30),
                    border: Border.all(color: scheme.primary.withAlpha(80)),
                  ),
                  child: Text(
                    entry.date.isEmpty ? '—' : entry.date,
                    style: Theme.of(context).textTheme.labelSmall?.copyWith(
                          color: scheme.primary,
                          fontWeight: FontWeight.w800,
                        ),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    entry.title,
                    style: Theme.of(context)
                        .textTheme
                        .titleMedium
                        ?.copyWith(fontWeight: FontWeight.w800),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 10),
            MarkdownView(text: entry.body),
          ],
        ),
      ),
    );
  }
}
