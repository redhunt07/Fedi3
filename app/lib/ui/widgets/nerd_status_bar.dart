/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../state/app_state.dart';

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

  int _lastTsMs = 0;
  int _lastRelayRx = 0;
  int _lastRelayTx = 0;
  int _lastP2pRx = 0;
  int _lastP2pTx = 0;
  int _lastMailboxRx = 0;
  int _lastMailboxTx = 0;
  int _lastWebrtcRx = 0;
  int _lastWebrtcTx = 0;

  double _relayDownBps = 0;
  double _relayUpBps = 0;
  double _p2pDownBps = 0;
  double _p2pUpBps = 0;
  double _mailboxDownBps = 0;
  double _mailboxUpBps = 0;
  double _webrtcDownBps = 0;
  double _webrtcUpBps = 0;

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
      });
      return;
    }

    setState(() => _loading = true);
    final api = CoreApi(config: cfg);
    try {
      final net = await api.fetchNetMetrics();
      final relay = (net['relay'] is Map) ? (net['relay'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
      final p2p = (net['p2p'] is Map) ? (net['p2p'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
      final mailbox = (net['mailbox'] is Map) ? (net['mailbox'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
      final webrtc = (net['webrtc'] is Map) ? (net['webrtc'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
      final ts = (net['ts_ms'] is num) ? (net['ts_ms'] as num).toInt() : DateTime.now().millisecondsSinceEpoch;

      final relayRx = (relay['rx_bytes'] is num) ? (relay['rx_bytes'] as num).toInt() : 0;
      final relayTx = (relay['tx_bytes'] is num) ? (relay['tx_bytes'] as num).toInt() : 0;
      final p2pRx = (p2p['rx_bytes'] is num) ? (p2p['rx_bytes'] as num).toInt() : 0;
      final p2pTx = (p2p['tx_bytes'] is num) ? (p2p['tx_bytes'] as num).toInt() : 0;
      final mailboxRx = (mailbox['rx_bytes'] is num) ? (mailbox['rx_bytes'] as num).toInt() : 0;
      final mailboxTx = (mailbox['tx_bytes'] is num) ? (mailbox['tx_bytes'] as num).toInt() : 0;
      final webrtcRx = (webrtc['rx_bytes'] is num) ? (webrtc['rx_bytes'] as num).toInt() : 0;
      final webrtcTx = (webrtc['tx_bytes'] is num) ? (webrtc['tx_bytes'] as num).toInt() : 0;

      var dtMs = ts - _lastTsMs;
      if (_lastTsMs == 0 || dtMs <= 0) dtMs = 2000;
      final dt = dtMs / 1000.0;

      final relayDown = (relayRx - _lastRelayRx).clamp(0, 1 << 62) / dt;
      final relayUp = (relayTx - _lastRelayTx).clamp(0, 1 << 62) / dt;
      final p2pDown = (p2pRx - _lastP2pRx).clamp(0, 1 << 62) / dt;
      final p2pUp = (p2pTx - _lastP2pTx).clamp(0, 1 << 62) / dt;
      final mailboxDown = (mailboxRx - _lastMailboxRx).clamp(0, 1 << 62) / dt;
      final mailboxUp = (mailboxTx - _lastMailboxTx).clamp(0, 1 << 62) / dt;
      final webrtcDown = (webrtcRx - _lastWebrtcRx).clamp(0, 1 << 62) / dt;
      final webrtcUp = (webrtcTx - _lastWebrtcTx).clamp(0, 1 << 62) / dt;

      if (!mounted) return;
      setState(() {
        _net = net;
        _lastTsMs = ts;
        _lastRelayRx = relayRx;
        _lastRelayTx = relayTx;
        _lastP2pRx = p2pRx;
        _lastP2pTx = p2pTx;
        _lastMailboxRx = mailboxRx;
        _lastMailboxTx = mailboxTx;
        _lastWebrtcRx = webrtcRx;
        _lastWebrtcTx = webrtcTx;
        _relayDownBps = relayDown.toDouble();
        _relayUpBps = relayUp.toDouble();
        _p2pDownBps = p2pDown.toDouble();
        _p2pUpBps = p2pUp.toDouble();
        _mailboxDownBps = mailboxDown.toDouble();
        _mailboxUpBps = mailboxUp.toDouble();
        _webrtcDownBps = webrtcDown.toDouble();
        _webrtcUpBps = webrtcUp.toDouble();
      });
    } catch (_) {
      if (!mounted) return;
      if (_net == null) {
        setState(() {
          _net = {
            'ts_ms': DateTime.now().millisecondsSinceEpoch,
            'relay': const <String, dynamic>{},
            'p2p': {'enabled': widget.appState.config?.p2pEnable ?? false},
            'mailbox': const <String, dynamic>{},
            'webrtc': const <String, dynamic>{},
          };
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

    final p2p = (_net?['p2p'] is Map) ? (_net!['p2p'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
    final p2pEnabled = (p2p['enabled'] == true);
    final p2pPeers = (p2p['connected_peers'] is num) ? (p2p['connected_peers'] as num).toInt() : 0;
    final p2pActive = (p2p['active_peers'] is num) ? (p2p['active_peers'] as num).toInt() : 0;
    final p2pRtt = (p2p['rtt_ms'] is num) ? (p2p['rtt_ms'] as num).toInt() : 0;
    final p2pColor = !p2pEnabled
        ? Colors.grey
        : (p2pPeers > 0)
            ? Colors.greenAccent
            : Colors.orangeAccent;

    final mailbox = (_net?['mailbox'] is Map) ? (_net!['mailbox'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
    final mailboxPeers = (mailbox['active_peers'] is num) ? (mailbox['active_peers'] as num).toInt() : 0;
    final mailboxRtt = (mailbox['rtt_ms'] is num) ? (mailbox['rtt_ms'] as num).toInt() : 0;
    final mailboxColor = !p2pEnabled
        ? Colors.grey
        : (mailboxPeers > 0)
            ? Colors.greenAccent
            : Colors.orangeAccent;

    final webrtc = (_net?['webrtc'] is Map) ? (_net!['webrtc'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
    final webrtcSessions = (webrtc['sessions'] is num) ? (webrtc['sessions'] as num).toInt() : 0;
    final webrtcPeers = (webrtc['active_peers'] is num) ? (webrtc['active_peers'] as num).toInt() : 0;
    final webrtcColor = !p2pEnabled
        ? Colors.grey
        : (webrtcSessions > 0 || webrtcPeers > 0)
            ? Colors.greenAccent
            : Colors.orangeAccent;

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
                  Tooltip(
                    message: context.l10n.statusRelay,
                    child: _chip(icon: Icons.cloud, color: relayColor, text: _relayLabel()),
                  ),
                  const SizedBox(width: 10),
              Tooltip(
                message: context.l10n.statusP2p,
                child: _chip(icon: Icons.wifi_tethering, color: p2pColor, text: _p2pLabel(p2pPeers, p2pActive)),
              ),
              const SizedBox(width: 10),
              Tooltip(
                message: context.l10n.statusMailbox,
                child: _chip(icon: Icons.markunread_mailbox, color: mailboxColor, text: mailboxPeers.toString()),
              ),
              const SizedBox(width: 10),
              Tooltip(
                message: context.l10n.statusWebrtc,
                child: _chip(icon: Icons.video_call, color: webrtcColor, text: '${webrtcSessions.toString()}·${webrtcPeers.toString()}'),
              ),
              const SizedBox(width: 16),
              _mono('${context.l10n.statusRelayRtt}: ${relayRtt > 0 ? '${relayRtt}ms' : '-'}', fg),
              const SizedBox(width: 10),
              _mono('${context.l10n.statusP2pRtt}: ${p2pRtt > 0 ? '${p2pRtt}ms' : '-'}', fg),
              const SizedBox(width: 10),
              _mono('${context.l10n.statusMailboxRtt}: ${mailboxRtt > 0 ? '${mailboxRtt}ms' : '-'}', fg),
              const SizedBox(width: 16),
              _mono('${context.l10n.statusRelayTraffic} ${_fmtRate(_relayUpBps)}/${_fmtRate(_relayDownBps)}', fg),
              const SizedBox(width: 10),
              _mono('${context.l10n.statusP2pTraffic} ${_fmtRate(_p2pUpBps)}/${_fmtRate(_p2pDownBps)}', fg),
              const SizedBox(width: 10),
              _mono('${context.l10n.statusMailboxTraffic} ${_fmtRate(_mailboxUpBps)}/${_fmtRate(_mailboxDownBps)}', fg),
              const SizedBox(width: 10),
              _mono('${context.l10n.statusWebrtcTraffic} ${_fmtRate(_webrtcUpBps)}/${_fmtRate(_webrtcDownBps)}', fg),
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

  String _p2pLabel(int connected, int active) {
    if (!widget.appState.isRunning) return context.l10n.statusCoreStoppedShort;
    if (_net?['p2p'] is! Map) return context.l10n.statusUnknownShort;
    if (connected <= 0 && active <= 0) return context.l10n.statusNoPeersShort;
    if (active <= 0) return '$connected';
    return '$connected·$active';
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
