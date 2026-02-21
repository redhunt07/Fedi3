/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';
import '../screens/networking_settings_screen.dart';

class NerdStatusBar extends StatefulWidget {
  const NerdStatusBar({super.key, required this.appState});

  final AppState appState;

  @override
  State<NerdStatusBar> createState() => _NerdStatusBarState();
}

class _NerdStatusBarState extends State<NerdStatusBar> {
  Timer? _timer;
  bool _loading = false;

  Map<String, dynamic>? _net;
  Map<String, dynamic>? _p2p;

  int _lastTsMs = 0;
  int _lastRelayRx = 0;
  int _lastRelayTx = 0;

  double _relayDownBps = 0;
  double _relayUpBps = 0;
  int _p2pConnected = 0;
  int _p2pActive = 0;
  bool _p2pEnabled = false;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 2), (_) => _poll());
    WidgetsBinding.instance.addPostFrameCallback((_) => _poll());
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _poll() async {
    if (_loading) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (!widget.appState.isRunning) {
      setState(() {
        _net = null;
        _p2p = null;
      });
      return;
    }

    setState(() => _loading = true);
    final api = CoreApi(config: cfg);
    try {
      Map<String, dynamic>? net;
      Map<String, dynamic>? p2p;
      try {
        net = await api.fetchNetMetrics();
      } catch (_) {
        net = null;
      }
      try {
        p2p = await api.fetchP2pDebug();
      } catch (_) {
        p2p = null;
      }
      if (net == null && p2p == null) {
        if (!mounted) return;
        if (_net == null) {
          setState(() {
            _net = {
              'ts_ms': DateTime.now().millisecondsSinceEpoch,
              'relay': const <String, dynamic>{},
            };
            _p2p = null;
            _p2pEnabled = false;
            _p2pActive = 0;
            _p2pConnected = 0;
          });
        }
        return;
      }

      if (net == null) {
        if (!mounted) return;
        setState(() {
          _p2p = p2p;
          _p2pEnabled = false;
          _p2pActive = 0;
          _p2pConnected = 0;
        });
        return;
      }

      final relay = (net['relay'] is Map) ? (net['relay'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
      final ts = (net['ts_ms'] is num) ? (net['ts_ms'] as num).toInt() : DateTime.now().millisecondsSinceEpoch;

      final relayRx = (relay['rx_bytes'] is num) ? (relay['rx_bytes'] as num).toInt() : 0;
      final relayTx = (relay['tx_bytes'] is num) ? (relay['tx_bytes'] as num).toInt() : 0;
      final p2pMap = (p2p?['p2p'] is Map) ? (p2p!['p2p'] as Map).cast<String, dynamic>() : const <String, dynamic>{};

      var dtMs = ts - _lastTsMs;
      if (_lastTsMs == 0 || dtMs <= 0) dtMs = 2000;
      final dt = dtMs / 1000.0;

      final relayDown = (relayRx - _lastRelayRx).clamp(0, 1 << 62) / dt;
      final relayUp = (relayTx - _lastRelayTx).clamp(0, 1 << 62) / dt;

      if (!mounted) return;
      setState(() {
        _net = net;
        _p2p = p2p;
        _lastTsMs = ts;
        _lastRelayRx = relayRx;
        _lastRelayTx = relayTx;
        _relayDownBps = relayDown.toDouble();
        _relayUpBps = relayUp.toDouble();
        _p2pConnected = (p2pMap['connected_peers'] is num) ? (p2pMap['connected_peers'] as num).toInt() : 0;
        _p2pActive = (p2pMap['active_peers'] is num) ? (p2pMap['active_peers'] as num).toInt() : 0;
        _p2pEnabled = p2pMap['enabled'] == true;
      });
    } catch (_) {
      if (!mounted) return;
      if (_net == null) {
        setState(() {
          _net = {
            'ts_ms': DateTime.now().millisecondsSinceEpoch,
            'relay': const <String, dynamic>{},
          };
          _p2p = null;
          _p2pEnabled = false;
          _p2pActive = 0;
          _p2pConnected = 0;
        });
      }
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    const h = 26.0;
    final fg = Theme.of(context).colorScheme.onSurface.withAlpha(170);
    final bg = Theme.of(context).colorScheme.surface.withAlpha(235);

    final relay = (_net?['relay'] is Map) ? (_net!['relay'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
    final relayConnected = (relay['connected'] == true);
    final relayRtt = (relay['rtt_ms'] is num) ? (relay['rtt_ms'] as num).toInt() : 0;
    final relayColor = !widget.appState.isRunning ? Colors.grey : (relayConnected ? Colors.greenAccent : Colors.redAccent);

    return Material(
      color: bg,
      child: SizedBox(
        height: h,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 10),
          decoration: BoxDecoration(
            border: Border(top: BorderSide(color: Theme.of(context).dividerColor.withAlpha(80))),
          ),
          child: ClipRect(
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                children: [
                  InkWell(
                    onTap: () => _openNetworking(context),
                    child: Tooltip(
                      message: context.l10n.statusRelay,
                      child: _chip(icon: Icons.cloud, color: relayColor, text: _relayLabel()),
                    ),
                  ),
                  const SizedBox(width: 10),
                  InkWell(
                    onTap: () => _openNetworking(context),
                    child: Tooltip(
                      message: 'P2P',
                      child: _chip(icon: Icons.hub, color: _p2pColor(), text: _p2pLabel()),
                    ),
                  ),
                  const SizedBox(width: 10),
                  _mono('${context.l10n.statusRelayRtt}: ${relayRtt > 0 ? '${relayRtt}ms' : '-'}', fg),
                  const SizedBox(width: 10),
                  _mono('${context.l10n.statusRelayTraffic} ${_fmtRate(_relayUpBps)}/${_fmtRate(_relayDownBps)}', fg),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  String _relayLabel() {
    if (!widget.appState.isRunning) return context.l10n.statusCoreStoppedShort;
    if (_net?['relay'] is! Map) return context.l10n.statusUnknownShort;
    final relay = (_net!['relay'] as Map).cast<String, dynamic>();
    final online = relay['connected'] == true;
    return online ? context.l10n.statusConnectedShort : context.l10n.statusDisconnectedShort;
  }

  String _p2pLabel() {
    if (!widget.appState.isRunning) return context.l10n.statusCoreStoppedShort;
    if (_p2p == null) return context.l10n.statusUnknownShort;
    if (!_p2pEnabled) return context.l10n.statusDisconnectedShort;
    if (_p2pConnected == 0 && _p2pActive == 0) return context.l10n.statusNoPeersShort;
    return '${_p2pActive.clamp(0, 999)}/${_p2pConnected.clamp(0, 999)}';
  }

  Color _p2pColor() {
    if (!widget.appState.isRunning) return Colors.grey;
    if (!_p2pEnabled) return Colors.orangeAccent;
    if (_p2pActive > 0) return Colors.greenAccent;
    return Colors.redAccent;
  }

  void _openNetworking(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute(builder: (_) => NetworkingSettingsScreen(appState: widget.appState)),
    );
  }

  Widget _chip({required IconData icon, required Color color, required String text}) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, size: 14, color: color),
        const SizedBox(width: 6),
        Text(
          text,
          style: const TextStyle(fontSize: 11, fontFeatures: [FontFeature.tabularFigures()]),
        ),
      ],
    );
  }

  Widget _mono(String text, Color fg) {
    return Text(
      text,
      style: TextStyle(
        fontSize: 11,
        color: fg,
        fontFeatures: const [FontFeature.tabularFigures()],
      ),
    );
  }

  String _fmtRate(double bps) {
    if (bps <= 0) return '0B/s';
    const k = 1024.0;
    if (bps < k) return '${bps.toStringAsFixed(0)}B/s';
    if (bps < k * k) return '${(bps / k).toStringAsFixed(1)}KB/s';
    if (bps < k * k * k) return '${(bps / (k * k)).toStringAsFixed(1)}MB/s';
    return '${(bps / (k * k * k)).toStringAsFixed(1)}GB/s';
  }
}
