/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/identity_sync.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';

/// Desktop side of the QR pairing flow: shows a QR with a short-lived code and
/// polls the relay until an already logged-in client uploads the sealed
/// identity package. The received package starts the local core with the same
/// account identity.
class PairingScreen extends StatefulWidget {
  const PairingScreen({
    super.key,
    required this.appState,
    this.initialInstanceUrl,
  });

  final AppState appState;

  /// Instance URL entered on the login screen or taken from the relay
  /// directory, used to derive the relay HTTP base for polling.
  final String? initialInstanceUrl;

  @override
  State<PairingScreen> createState() => _PairingScreenState();
}

class _PairingScreenState extends State<PairingScreen> {
  static const Duration _pollInterval = Duration(seconds: 2);
  static const Duration _sessionDuration = Duration(minutes: 10);

  late final TextEditingController _instanceUrl;
  String? _code;
  String? _relayBase;
  Timer? _pollTimer;
  DateTime? _deadline;
  bool _polling = false;
  bool _starting = false;
  String? _error;
  String? _status;
  int _generation = 0;

  @override
  void initState() {
    super.initState();
    final initial = widget.initialInstanceUrl?.trim() ?? '';
    _instanceUrl = TextEditingController(
      text: initial.isNotEmpty
          ? initial
          : (widget.appState.config?.publicBaseUrl ?? ''),
    );
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _instanceUrl.text.trim().isNotEmpty) {
        unawaited(_startPairing());
      }
    });
  }

  @override
  void dispose() {
    _pollTimer?.cancel();
    _instanceUrl.dispose();
    super.dispose();
  }

  String _randomCode() {
    final random = Random.secure();
    final bytes = List<int>.generate(16, (_) => random.nextInt(256));
    return bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  }

  Uri get _qrUri => Uri(
        scheme: 'fedi3',
        host: 'pair',
        queryParameters: {
          'relay': _relayBase ?? '',
          'code': _code ?? '',
        },
      );

  Future<void> _startPairing() async {
    final relayBase =
        IdentitySync.relayHttpBaseFromWs(_instanceUrl.text.trim());
    if (relayBase.isEmpty) {
      setState(() {
        _error = context.l10n.pairingInvalidUrl;
        _status = null;
      });
      return;
    }
    _pollTimer?.cancel();
    _generation++;
    final generation = _generation;
    setState(() {
      _relayBase = relayBase;
      _code = _randomCode();
      _deadline = DateTime.now().add(_sessionDuration);
      _error = null;
      _status = context.l10n.pairingWaiting;
      _starting = false;
    });
    _pollTimer = Timer.periodic(_pollInterval, (_) {
      if (!mounted || generation != _generation) return;
      unawaited(_pollOnce(generation));
    });
    unawaited(_pollOnce(generation));
  }

  void _stopPolling() {
    _pollTimer?.cancel();
    _pollTimer = null;
  }

  Future<void> _pollOnce(int generation) async {
    if (!mounted ||
        generation != _generation ||
        _polling ||
        _starting ||
        _code == null ||
        _relayBase == null) {
      return;
    }
    final deadline = _deadline;
    if (deadline != null && DateTime.now().isAfter(deadline)) {
      _stopPolling();
      setState(() {
        _status = null;
        _error = context.l10n.pairingExpired;
      });
      return;
    }
    _polling = true;
    Map<String, dynamic>? sealed;
    try {
      sealed = await IdentitySync.fetchPairingPackage(
        relayBase: _relayBase!,
        code: _code!,
      );
    } catch (_) {
      // A transient relay error must not end the session: keep polling.
      sealed = null;
    } finally {
      _polling = false;
    }
    if (!mounted || generation != _generation || sealed == null) return;

    _stopPolling();
    setState(() {
      _starting = true;
      _error = null;
      _status = context.l10n.pairingConnected;
    });
    try {
      if (!IdentitySync.looksSealed(sealed)) {
        throw const FormatException('invalid pairing envelope');
      }
      final package = await IdentitySync.open(sealed, _code!);
      final config = IdentitySync.configFromPackage(package);
      // replaceConfig stops a running core first, so pairing also works from
      // the settings screen of an already logged-in client.
      await widget.appState.replaceConfig(config);
      if (!mounted) return;
      if (!widget.appState.isRunning) {
        setState(() {
          _error = context.l10n.pairingError(
              widget.appState.lastError ?? context.l10n.networkErrorTitle);
          _starting = false;
        });
        return;
      }
      // The snapshot is only carried by the pairing package: it is not part of
      // CoreConfig. Importing it is best-effort, pairing still succeeds.
      final rawSnapshot = package['chat_snapshot'];
      final chatSnapshot =
          rawSnapshot is Map ? rawSnapshot.cast<String, dynamic>() : null;
      if (chatSnapshot != null && chatSnapshot.isNotEmpty) {
        try {
          await CoreApi(config: config).importChatSnapshot(chatSnapshot);
        } catch (e) {
          // Identity pairing already succeeded; history can be rebuilt from
          // the relay spool later.
          debugPrint('Fedi3 pairing: chat snapshot import failed: $e');
        }
      }
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = context.l10n.pairingError(e.toString());
        _starting = false;
      });
    }
  }

  void _cancel() {
    _stopPolling();
    Navigator.of(context).pop(false);
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.pairingTitle)),
      body: ListView(
        padding: const EdgeInsets.all(UiTokens.padScreen),
        children: [
          const Center(child: BrandMark(size: 56)),
          const SizedBox(height: 14),
          Text(
            context.l10n.pairingIntro,
            textAlign: TextAlign.center,
            style: TextStyle(color: scheme.onSurfaceVariant),
          ),
          const SizedBox(height: 18),
          GlassPanel(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                TextField(
                  key: const Key('pairingInstanceUrlField'),
                  controller: _instanceUrl,
                  keyboardType: TextInputType.url,
                  autocorrect: false,
                  enabled: !_starting,
                  decoration: InputDecoration(
                    labelText: context.l10n.pairingInstanceUrl,
                  ),
                ),
                const SizedBox(height: 12),
                AuroraButton(
                  label: _code == null
                      ? context.l10n.pairingGenerate
                      : context.l10n.pairingRegenerate,
                  onPressed: _starting ? null : _startPairing,
                  icon: Icons.qr_code_2_rounded,
                  expand: true,
                ),
              ],
            ),
          ),
          if (_code != null && _relayBase != null) ...[
            const SizedBox(height: 18),
            Center(
              child: Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: Colors.white,
                  borderRadius: BorderRadius.circular(18),
                  boxShadow: [
                    BoxShadow(
                      color: scheme.primary.withAlpha(40),
                      blurRadius: 24,
                    ),
                  ],
                ),
                child: QrImageView(
                  data: _qrUri.toString(),
                  size: 220,
                  backgroundColor: Colors.white,
                  eyeStyle: const QrEyeStyle(
                    eyeShape: QrEyeShape.square,
                    color: Colors.black,
                  ),
                  dataModuleStyle: const QrDataModuleStyle(
                    dataModuleShape: QrDataModuleShape.square,
                    color: Colors.black,
                  ),
                ),
              ),
            ),
            const SizedBox(height: 12),
            Center(
              child: Text(
                _relayBase!,
                style: TextStyle(fontSize: 11, color: scheme.onSurfaceVariant),
              ),
            ),
          ],
          if (_status != null) ...[
            const SizedBox(height: 14),
            Center(
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (_starting)
                    const SizedBox(
                      width: 14,
                      height: 14,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    ),
                  if (_starting) const SizedBox(width: 8),
                  Flexible(child: Text(_status!)),
                ],
              ),
            ),
          ],
          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(
              _error!,
              textAlign: TextAlign.center,
              style: TextStyle(color: scheme.error),
            ),
          ],
          const SizedBox(height: 18),
          OutlinedButton.icon(
            onPressed: _cancel,
            icon: const Icon(Icons.close_rounded),
            label: Text(context.l10n.cancel),
          ),
        ],
      ),
    );
  }
}
