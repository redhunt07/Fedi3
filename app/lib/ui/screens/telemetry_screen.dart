/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';
import 'package:share_plus/share_plus.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/telemetry_service.dart';

class TelemetryScreen extends StatefulWidget {
  const TelemetryScreen({super.key});

  @override
  State<TelemetryScreen> createState() => _TelemetryScreenState();
}

class _TelemetryScreenState extends State<TelemetryScreen> {
  bool _loading = true;
  List<TelemetryEvent> _events = const [];

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() => _loading = true);
    final items = await TelemetryService.loadRecent(limit: 200);
    if (!mounted) return;
    setState(() {
      _events = items;
      _loading = false;
    });
  }

  Future<void> _clear() async {
    await TelemetryService.clear();
    await _load();
  }

  Future<void> _export() async {
    final l10n = context.l10n;
    final file = await TelemetryService.exportLog();
    if (file == null) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(l10n.telemetryExportEmpty)),
      );
      return;
    }
    if (!context.mounted) return;
    await Share.shareXFiles([XFile(file.path)], subject: l10n.telemetryTitle);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.telemetryTitle),
        actions: [
          IconButton(
            tooltip: context.l10n.telemetryRefresh,
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
          IconButton(
            tooltip: context.l10n.telemetryExport,
            onPressed: _loading ? null : _export,
            icon: const Icon(Icons.upload_file_outlined),
          ),
          IconButton(
            tooltip: context.l10n.telemetryClear,
            onPressed: _loading ? null : _clear,
            icon: const Icon(Icons.delete_outline),
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _events.isEmpty
              ? Center(child: Text(context.l10n.telemetryEmpty))
              : ListView.separated(
                  padding: const EdgeInsets.all(16),
                  itemCount: _events.length,
                  separatorBuilder: (_, __) => const SizedBox(height: 8),
                  itemBuilder: (context, index) {
                    final ev = _events[index];
                    return Card(
                      child: Padding(
                        padding: const EdgeInsets.all(12),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '${ev.type} · ${ev.ts.toLocal().toIso8601String()}',
                              style: const TextStyle(fontWeight: FontWeight.w700),
                            ),
                            const SizedBox(height: 6),
                            Text(ev.message),
                            if (ev.data != null && ev.data!.isNotEmpty) ...[
                              const SizedBox(height: 8),
                              Text(
                                ev.data.toString(),
                                style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(150)),
                              ),
                            ],
                          ],
                        ),
                      ),
                    );
                  },
                ),
    );
  }
}
