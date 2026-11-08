import '../../services/app_activity.dart';
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../utils/time_ago.dart';
import '../widgets/network_error_card.dart';

class RelaysScreen extends StatefulWidget {
  const RelaysScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<RelaysScreen> createState() => _RelaysScreenState();
}

class _RelaysScreenState extends State<RelaysScreen> {
  bool _loading = false;
  String? _error;
  Map<String, dynamic>? _stats;
  Map<String, dynamic>? _coverage;
  List<dynamic>? _relays;
  List<dynamic>? _peers;
  String _peerQuery = '';
  final Map<String, int> _relayLatencyMs = {};
  String? _recommendedRelay;
  Timer? _peerRefresh;
  bool _loadingPeers = false;
  bool _rawTelemetryOpen = false;

  @override
  void initState() {
    super.initState();
    _refresh();
    _peerRefresh = AppActivity.periodic(
        const Duration(seconds: 10), (_) => _refreshPeers());
  }

  @override
  void dispose() {
    _peerRefresh?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    final cfg = widget.appState.config!;
    final api = CoreApi(config: cfg);
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final stats = await api.fetchRelayStats();
      final list = await api.fetchRelayList();
      final coverage = await api.fetchRelayCoverage();
      final peers = await api.fetchRelayPeers(query: _peerQuery);
      final relays = (list['relays'] as List<dynamic>? ?? []);
      final latency = await _measureRelayLatency(relays);
      if (!mounted) return;
      setState(() {
        _stats = stats;
        _relays = relays;
        _coverage = coverage;
        _peers = (peers['items'] as List<dynamic>? ?? []);
        _relayLatencyMs
          ..clear()
          ..addAll(latency);
        _recommendedRelay = _pickRecommendedRelay(latency);
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _refreshPeers({bool showLoading = false}) async {
    if (_loadingPeers) return;
    if (!mounted) return;
    final cfg = widget.appState.config!;
    final api = CoreApi(config: cfg);
    _loadingPeers = true;
    if (showLoading) {
      setState(() {});
    }
    try {
      final peers = await api.fetchRelayPeers(query: _peerQuery);
      if (!mounted) return;
      setState(() {
        _peers = (peers['items'] as List<dynamic>? ?? []);
      });
    } catch (_) {
      // Best-effort: keep existing list if refresh fails.
    } finally {
      _loadingPeers = false;
      if (showLoading && mounted) {
        setState(() {});
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config!;
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.relaysTitle),
        actions: [
          IconButton(
              onPressed: _loading ? null : _refresh,
              icon: const Icon(Icons.refresh)),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Card(
            child: ListTile(
              title: Text(context.l10n.relaysCurrent),
              subtitle: Text(cfg.publicBaseUrl),
            ),
          ),
          if (_error != null)
            NetworkErrorCard(
              message: _error,
              onRetry: _refresh,
              compact: true,
            ),
          if (_stats != null) ...[
            const SizedBox(height: 10),
            _telemetryCard(context, _stats!),
          ],
          if (_coverage != null) ...[
            const SizedBox(height: 10),
            _coverageCard(context, _coverage!),
          ],
          const SizedBox(height: 10),
          Text(context.l10n.relaysKnown,
              style: const TextStyle(fontWeight: FontWeight.w700)),
          const SizedBox(height: 8),
          if (_loading)
            const Center(
                child: Padding(
                    padding: EdgeInsets.all(16),
                    child: CircularProgressIndicator())),
          for (final r in (_relays ?? const [])) _relayTile(context, r as Map),
          const SizedBox(height: 16),
          Text(context.l10n.relaysPeersTitle,
              style: const TextStyle(fontWeight: FontWeight.w700)),
          const SizedBox(height: 8),
          TextField(
            decoration: InputDecoration(
              prefixIcon: const Icon(Icons.search),
              hintText: context.l10n.relaysPeersSearchHint,
            ),
            onChanged: (value) {
              _peerQuery = value;
              if (!_loading) {
                _refreshPeers(showLoading: true);
              }
            },
          ),
          const SizedBox(height: 8),
          if (_loading || _loadingPeers)
            const Center(
                child: Padding(
                    padding: EdgeInsets.all(16),
                    child: CircularProgressIndicator())),
          if (!_loading &&
              !_loadingPeers &&
              (_peers == null || _peers!.isEmpty))
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 12),
              child: Text(
                context.l10n.relaysPeersEmpty,
                style: TextStyle(
                    color:
                        Theme.of(context).colorScheme.onSurface.withAlpha(160)),
              ),
            ),
          for (final p in (_peers ?? const []))
            Card(
              child: ListTile(
                leading: _peerAvatar(
                  context,
                  (p as Map)['username']?.toString() ?? '',
                  (p)['online'] == true,
                ),
                title: Text(p['username']?.toString() ?? ''),
                subtitle: Text((p)['actor_url']?.toString() ?? ''),
                trailing: Text((p)['peer_id']?.toString() ?? ''),
              ),
            ),
        ],
      ),
    );
  }

  Widget _relayTile(BuildContext context, Map relay) {
    final url = relay['relay_url']?.toString() ?? '';
    final lastSeenMs = (relay['last_seen_ms'] as num?)?.toInt();
    final lastSeen = lastSeenMs == null
        ? null
        : formatTimeAgo(
            context, DateTime.fromMillisecondsSinceEpoch(lastSeenMs));
    final telemetry = relay['telemetry'];
    final latency = _relayLatencyMs[url];
    final coverageText = _coverageFromTelemetry(context, telemetry);
    final isRecommended = _recommendedRelay == url && latency != null;
    return Card(
      child: ListTile(
        title: Wrap(
          spacing: 6,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Text(url),
            if (isRecommended)
              Chip(label: Text(context.l10n.relaysRecommended)),
          ],
        ),
        subtitle: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (lastSeen != null)
              Text(context.l10n.relaysLastSeenLabel(lastSeen)),
            if (latency != null) Text(context.l10n.relaysLatency(latency)),
            if (coverageText != null) Text(coverageText),
          ],
        ),
      ),
    );
  }

  Widget _peerAvatar(BuildContext context, String username, bool online) {
    final initials = username.isNotEmpty ? username[0].toUpperCase() : '?';
    final statusColor = online ? Colors.green : Colors.grey;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        CircleAvatar(
          radius: 18,
          child: Text(initials),
        ),
        Positioned(
          right: -1,
          bottom: -1,
          child: Container(
            width: 10,
            height: 10,
            decoration: BoxDecoration(
              color: statusColor,
              shape: BoxShape.circle,
              border: Border.all(
                color: Theme.of(context).colorScheme.surface,
                width: 2,
              ),
            ),
          ),
        ),
      ],
    );
  }


  Widget _telemetryCard(BuildContext context, Map<String, dynamic> stats) {
    // The core returns the telemetry object at the top level; older payloads
    // nested it under "telemetry".
    final telemetry = stats['telemetry'] is Map
        ? (stats['telemetry'] as Map).cast<String, dynamic>()
        : stats;
    int? count(String key) => (telemetry[key] as num?)?.toInt();
    final relays = telemetry['relays'];
    final relayCount = relays is List ? relays.length : null;
    final measured = count('timestamp_ms');
    final scheme = Theme.of(context).colorScheme;

    final metrics = <(String, String)>[
      if (count('online_users') != null)
        (context.l10n.relaysOnlineUsers, '${count('online_users')}'),
      if (count('online_peers') != null)
        (context.l10n.relaysOnlinePeers, '${count('online_peers')}'),
      if (count('total_users') != null)
        (context.l10n.relaysTotalUsers, '${count('total_users')}'),
      if (count('total_peers_seen') != null)
        (context.l10n.relaysPeersSeen, '${count('total_peers_seen')}'),
      if (relayCount != null)
        (context.l10n.relaysRelayCount, '$relayCount'),
    ];

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.monitor_heart_outlined, color: scheme.primary),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    context.l10n.relaysTelemetry,
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                ),
                if (measured != null)
                  Text(
                    DateTime.fromMillisecondsSinceEpoch(measured)
                        .toLocal()
                        .toString()
                        .substring(11, 16),
                    style: TextStyle(
                      color: scheme.onSurface.withAlpha(150),
                      fontSize: 12,
                    ),
                  ),
              ],
            ),
            if (metrics.isNotEmpty) ...[
              const SizedBox(height: 12),
              for (final m in metrics)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 3),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          m.$1,
                          style: TextStyle(
                            fontSize: 13,
                            color: scheme.onSurface.withAlpha(190),
                          ),
                        ),
                      ),
                      Text(
                        m.$2,
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w700,
                          color: scheme.primary,
                        ),
                      ),
                    ],
                  ),
                ),
            ],
            const SizedBox(height: 4),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                onPressed: () =>
                    setState(() => _rawTelemetryOpen = !_rawTelemetryOpen),
                child: Text(context.l10n.relaysViewRaw),
              ),
            ),
            if (_rawTelemetryOpen)
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: scheme.surfaceContainerHighest,
                  borderRadius: BorderRadius.circular(UiTokens.radiusCardSm),
                ),
                child: SelectableText(
                  const JsonEncoder.withIndent('  ').convert(telemetry),
                  style: const TextStyle(fontSize: 11, height: 1.35),
                ),
              ),
          ],
        ),
      ),
    );
  }

  String _relativeMs(BuildContext context, Object? raw) {
    final ms = (raw as num?)?.toInt();
    if (ms == null || ms <= 0) return '-';
    return formatTimeAgo(context, DateTime.fromMillisecondsSinceEpoch(ms));
  }

  String? _coverageFromTelemetry(BuildContext context, Object? telemetry) {
    if (telemetry is! Map) return null;
    final indexed = (telemetry['search_indexed_users'] as num?)?.toInt();
    final total = (telemetry['search_total_users'] as num?)?.toInt();
    if (indexed == null || total == null) return null;
    return context.l10n.relaysCoverageUsers(indexed, total);
  }

  Widget _coverageCard(BuildContext context, Map<String, dynamic> coverage) {
    final total = (coverage['total_users'] as num?)?.toDouble() ?? 0;
    final indexed = (coverage['indexed_users'] as num?)?.toDouble() ?? 0;
    final ratio = total <= 0 ? 0.0 : (indexed / total).clamp(0.0, 1.0);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.l10n.relaysCoverageTitle,
                style: const TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 6),
            Text(context.l10n
                .relaysCoverageUsers(indexed.toInt(), total.toInt())),
            const SizedBox(height: 8),
            LinearProgressIndicator(value: ratio),
            const SizedBox(height: 6),
            Text(
              context.l10n.relaysCoverageLast(
                _relativeMs(context, coverage['last_index_ms']),
              ),
              style: TextStyle(
                  color: Theme.of(context).colorScheme.onSurface.withAlpha(160),
                  fontSize: 12),
            ),
          ],
        ),
      ),
    );
  }

  String? _pickRecommendedRelay(Map<String, int> latency) {
    if (latency.isEmpty) return null;
    String? best;
    int? bestMs;
    latency.forEach((url, ms) {
      if (bestMs == null || ms < bestMs!) {
        best = url;
        bestMs = ms;
      }
    });
    return best;
  }

  Future<Map<String, int>> _measureRelayLatency(List<dynamic> relays) async {
    final tasks = <Future<MapEntry<String, int>?>>[];
    for (final r in relays) {
      if (r is! Map) continue;
      final url = r['relay_url']?.toString();
      if (url == null || url.isEmpty) continue;
      tasks.add(_pingRelay(url));
    }
    final entries = await Future.wait(tasks);
    final out = <String, int>{};
    for (final entry in entries) {
      if (entry != null) {
        out[entry.key] = entry.value;
      }
    }
    return out;
  }

  Future<MapEntry<String, int>?> _pingRelay(String url) async {
    final uri = Uri.tryParse(url);
    if (uri == null) return null;
    final pingUri = uri.replace(path: '/healthz');
    final sw = Stopwatch()..start();
    try {
      final resp = await http.get(pingUri).timeout(const Duration(seconds: 3));
      sw.stop();
      if (resp.statusCode >= 200 && resp.statusCode < 300) {
        return MapEntry(url, sw.elapsedMilliseconds);
      }
    } catch (_) {
      sw.stop();
    }
    return null;
  }
}
