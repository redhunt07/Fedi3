/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/relay_admin_api.dart';
import '../../state/app_state.dart';
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
  RelayAdminApi? _adminApi;

  @override
  void initState() {
    super.initState();
    _initAdminApi();
    _refresh();
    _peerRefresh = Timer.periodic(const Duration(seconds: 10), (_) => _refreshPeers());
  }

  @override
  void didUpdateWidget(RelaysScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(widget.appState.config, oldWidget.appState.config)) {
      _initAdminApi();
    }
  }

  void _initAdminApi() {
    final token = widget.appState.prefs.relayAdminToken.trim();
    if (token.isNotEmpty) {
      _adminApi = RelayAdminApi(
        relayWs: widget.appState.config!.relayWs,
        adminToken: token,
      );
    } else {
      _adminApi = null;
    }
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


  String? _relayBaseFromConfig(CoreConfig cfg) {
    final raw = cfg.relayWs.trim();
    if (raw.isEmpty) return null;
    try {
      var uri = Uri.parse(raw);
      var scheme = uri.scheme;
      if (scheme == 'wss') {
        scheme = 'https';
      } else if (scheme == 'ws') {
        scheme = 'http';
      }
      if (scheme != 'http' && scheme != 'https') return null;
      uri = uri.replace(scheme: scheme, path: '', query: '', fragment: '');
      return uri.toString().replaceAll(RegExp(r'/+$'), '');
    } catch (_) {
      return null;
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
          IconButton(onPressed: _loading ? null : _refresh, icon: const Icon(Icons.refresh)),
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
            Card(
              child: ListTile(
                title: Text(context.l10n.relaysTelemetry),
                subtitle: Text(_stats.toString()),
              ),
            ),
          ],
          if (_coverage != null) ...[
            const SizedBox(height: 10),
            _coverageCard(context, _coverage!),
          ],
          const SizedBox(height: 10),
          Text(context.l10n.relaysKnown, style: const TextStyle(fontWeight: FontWeight.w700)),
          const SizedBox(height: 8),
          if (_loading) const Center(child: Padding(padding: EdgeInsets.all(16), child: CircularProgressIndicator())),
          for (final r in (_relays ?? const [])) _relayTile(context, r),
          const SizedBox(height: 16),
          Text(context.l10n.relaysPeersTitle, style: const TextStyle(fontWeight: FontWeight.w700)),
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
            const Center(child: Padding(padding: EdgeInsets.all(16), child: CircularProgressIndicator())),
          if (!_loading && !_loadingPeers && (_peers == null || _peers!.isEmpty))
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 12),
              child: Text(
                context.l10n.relaysPeersEmpty,
                style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
              ),
            ),
          for (final p in (_peers ?? const []))
            Card(
              child: ListTile(
                leading: _peerAvatar(
                  context,
                  (p)['username']?.toString() ?? '',
                  (p)['online'] == true,
                ),
                title: Text((p)['username']?.toString() ?? ''),
                subtitle: Text((p)['actor_url']?.toString() ?? ''),
                trailing: _adminApi != null
                    ? Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text((p)['peer_id']?.toString() ?? ''),
                          const SizedBox(width: 8),
                          IconButton(
                            icon: const Icon(Icons.delete_outline),
                            tooltip: 'Delete peer',
                            onPressed: _loadingPeers ? null : () => _deletePeer(p),
                          ),
                        ],
                      )
                    : Text((p)['peer_id']?.toString() ?? ''),
              ),
            ),
        ],
      ),
    );
  }

  Widget _relayTile(BuildContext context, Map relay) {
    final url = relay['relay_url']?.toString() ?? '';
    final lastSeen = relay['last_seen_ms']?.toString() ?? '';
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
            if (isRecommended) Chip(label: Text(context.l10n.relaysRecommended)),
          ],
        ),
        subtitle: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.l10n.relaysLastSeen(lastSeen)),
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
            Text(context.l10n.relaysCoverageTitle, style: const TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 6),
            Text(context.l10n.relaysCoverageUsers(indexed.toInt(), total.toInt())),
            const SizedBox(height: 8),
            LinearProgressIndicator(value: ratio),
            const SizedBox(height: 6),
            Text(
              context.l10n.relaysCoverageLast(coverage['last_index_ms']?.toString() ?? '-'),
              style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160), fontSize: 12),
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

  Future<void> _deletePeer(Map peer) async {
    final api = _adminApi;
    if (api == null) return;
    final peerId = peer['peer_id']?.toString() ?? '';
    if (peerId.isEmpty) return;
    final username = peer['username']?.toString() ?? '';
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete Peer'),
        content: Text('Are you sure you want to delete peer "$username" ($peerId)?'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancel')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Delete')),
        ],
      ),
    );
    if (confirmed != true) return;
    setState(() {
      _loadingPeers = true;
      _error = null;
    });
    try {
      await api.deletePeer(peerId);
      await _refreshPeers(showLoading: true);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loadingPeers = false);
    }
  }
}
