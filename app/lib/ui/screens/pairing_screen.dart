/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:math';

import 'package:material_ui/material_ui.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/identity_sync.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';

/// Source side of the QR login flow: the already logged-in client shows a QR
/// with a short-lived code, exports its account identity (and a chat snapshot),
/// seals the package with that code and uploads it to the relay. The device that
/// scans the QR fetches the package and starts with the same account.
class PairingScreen extends StatefulWidget {
  const PairingScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<PairingScreen> createState() => _PairingScreenState();
}

class _PairingScreenState extends State<PairingScreen> {
  static const Duration _sessionDuration = Duration(minutes: 10);

  String? _code;
  String? _relayBase;
  Timer? _expiryTimer;
  bool _busy = false;
  bool _ready = false;
  String? _error;
  String? _status;
  int _generation = 0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) unawaited(_startPairing());
    });
  }

  @override
  void dispose() {
    _expiryTimer?.cancel();
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

  /// Exports this client's identity and publishes the sealed package under a
  /// fresh code, then reveals the QR. The QR is shown only after the upload
  /// succeeded, so a scan always finds the package on the relay.
  Future<void> _startPairing() async {
    final config = widget.appState.config;
    if (config == null || !widget.appState.isRunning) {
      setState(() {
        _code = null;
        _ready = false;
        _busy = false;
        _status = null;
        _error = context.l10n.pairingLoginFirst;
      });
      return;
    }
    final relayBase = IdentitySync.relayHttpBaseFromWs(config.relayWs);
    if (relayBase.isEmpty) {
      setState(() {
        _code = null;
        _ready = false;
        _busy = false;
        _status = null;
        _error = context.l10n.pairingInvalidUrl;
      });
      return;
    }
    _expiryTimer?.cancel();
    _generation++;
    final generation = _generation;
    final code = _randomCode();
    setState(() {
      _code = code;
      _relayBase = relayBase;
      _ready = false;
      _busy = true;
      _error = null;
      _status = context.l10n.pairingPreparing;
    });
    _expiryTimer = Timer(_sessionDuration, () {
      if (!mounted || generation != _generation) return;
      setState(() {
        _ready = false;
        _status = null;
        _error = context.l10n.pairingExpired;
      });
    });
    try {
      final api = CoreApi(
        config: config,
        totalTimeout: const Duration(seconds: 120),
      );
      final exported = await api.exportIdentity();
      // History is a best-effort extra: the pairing must succeed even when the
      // snapshot endpoint is unavailable or the account has no chat yet.
      Map<String, dynamic>? chatSnapshot;
      try {
        chatSnapshot = await api.exportChatSnapshot();
      } catch (_) {
        chatSnapshot = null;
      }
      final package = IdentitySync.buildPackage(
        config,
        exported,
        chatSnapshot: chatSnapshot,
      );
      final envelope = await IdentitySync.seal(package, code);
      try {
        await IdentitySync.uploadPairingPackage(
          relayBase: relayBase,
          code: code,
          user: config.username,
          token: config.relayToken,
          envelope: envelope,
        );
      } catch (_) {
        // The relay may reject a large snapshot envelope (e.g. 400): retry
        // once without it so the identity pairing still succeeds.
        if (chatSnapshot == null || chatSnapshot.isEmpty) rethrow;
        final fallbackPackage = IdentitySync.buildPackage(config, exported);
        final fallbackEnvelope = await IdentitySync.seal(fallbackPackage, code);
        await IdentitySync.uploadPairingPackage(
          relayBase: relayBase,
          code: code,
          user: config.username,
          token: config.relayToken,
          envelope: fallbackEnvelope,
        );
      }
      if (!mounted || generation != _generation) return;
      debugPrint('[pair] QR ready code=$code relay=$relayBase');
      setState(() {
        _ready = true;
        _busy = false;
        _status = context.l10n.pairingWaiting;
      });
    } catch (e) {
      if (!mounted || generation != _generation) return;
      setState(() {
        _ready = false;
        _busy = false;
        _status = null;
        _error = context.l10n.pairingError(e.toString());
      });
    }
  }

  void _cancel() {
    _expiryTimer?.cancel();
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
          if (_ready && _code != null && _relayBase != null)
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
                  size: 240,
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
            )
          else
            const Center(
              child: SizedBox(
                width: 240,
                height: 240,
                child: Center(child: CircularProgressIndicator()),
              ),
            ),
          if (_ready && _relayBase != null) ...[
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
                  if (_busy)
                    const SizedBox(
                      width: 14,
                      height: 14,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    ),
                  if (_busy) const SizedBox(width: 8),
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
          AuroraButton(
            label: context.l10n.pairingRegenerate,
            onPressed: _busy ? null : _startPairing,
            icon: Icons.refresh_rounded,
            expand: true,
          ),
          const SizedBox(height: 8),
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
