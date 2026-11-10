/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/identity_sync.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';

/// Mobile side of the QR pairing flow: scans the code shown by the new client,
/// exports the local account identity, seals it with the pairing code and
/// uploads it to the relay so the new client can start with the same identity.
class LinkDeviceScreen extends StatefulWidget {
  const LinkDeviceScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<LinkDeviceScreen> createState() => _LinkDeviceScreenState();
}

class _LinkDeviceScreenState extends State<LinkDeviceScreen> {
  MobileScannerController? _controller;
  bool _busy = false;
  bool _done = false;
  String? _status;
  String? _error;
  String? _lastInvalidCode;

  bool get _scannerSupported => Platform.isAndroid || Platform.isIOS;

  @override
  void initState() {
    super.initState();
    if (_scannerSupported) {
      _controller = MobileScannerController(
        formats: const [BarcodeFormat.qrCode],
      );
    }
  }

  @override
  void dispose() {
    final controller = _controller;
    if (controller != null) {
      unawaited(controller.dispose());
    }
    super.dispose();
  }

  Future<void> _onDetect(BarcodeCapture capture) async {
    if (_busy || _done) return;
    for (final barcode in capture.barcodes) {
      final raw = barcode.rawValue?.trim() ?? '';
      if (raw.isEmpty) continue;
      final uri = Uri.tryParse(raw);
      if (uri == null ||
          uri.scheme.toLowerCase() != 'fedi3' ||
          uri.host.toLowerCase() != 'pair') {
        continue;
      }
      final relayRaw = uri.queryParameters['relay']?.trim() ?? '';
      final code = uri.queryParameters['code']?.trim() ?? '';
      final relayBase = IdentitySync.relayHttpBaseFromWs(relayRaw);
      if (relayBase.isEmpty || code.isEmpty) {
        if (_lastInvalidCode != raw) {
          _lastInvalidCode = raw;
          if (mounted) {
            setState(() => _error = context.l10n.linkDeviceInvalidCode);
          }
        }
        continue;
      }
      await _link(relayBase: relayBase, code: code);
      return;
    }
  }

  Future<void> _link({
    required String relayBase,
    required String code,
  }) async {
    final config = widget.appState.config;
    if (config == null) return;
    setState(() {
      _busy = true;
      _error = null;
      _status = null;
    });
    try {
      await _controller?.stop();
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
      if (!mounted) return;
      setState(() {
        _done = true;
        _busy = false;
        _status = context.l10n.linkDeviceSuccess;
      });
      await Future<void>.delayed(const Duration(milliseconds: 400));
      if (mounted) Navigator.of(context).pop(true);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _status = null;
        _error = context.l10n.linkDeviceError(e.toString());
      });
      unawaited(_controller?.start());
    }
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.linkDeviceTitle)),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(UiTokens.padScreen),
            child: Text(
              _scannerSupported
                  ? context.l10n.linkDeviceIntro
                  : context.l10n.linkDeviceUnsupported,
              textAlign: TextAlign.center,
              style: TextStyle(color: scheme.onSurfaceVariant),
            ),
          ),
          Expanded(
            child: !_scannerSupported
                ? Center(
                    child: Icon(Icons.qr_code_scanner_rounded,
                        size: 72, color: scheme.onSurfaceVariant),
                  )
                : Stack(
                    fit: StackFit.expand,
                    children: [
                      MobileScanner(
                        controller: _controller,
                        onDetect: (capture) {
                          unawaited(_onDetect(capture));
                        },
                      ),
                      if (_busy)
                        Container(
                          color: Colors.black.withAlpha(80),
                          alignment: Alignment.center,
                          child: const CircularProgressIndicator(),
                        ),
                    ],
                  ),
          ),
          Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              children: [
                if (_status != null)
                  Text(_status!, textAlign: TextAlign.center),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Text(
                      _error!,
                      textAlign: TextAlign.center,
                      style: TextStyle(color: scheme.error),
                    ),
                  ),
                const SizedBox(height: 10),
                OutlinedButton.icon(
                  onPressed:
                      _done ? null : () => Navigator.of(context).pop(false),
                  icon: const Icon(Icons.close_rounded),
                  label: Text(context.l10n.cancel),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
