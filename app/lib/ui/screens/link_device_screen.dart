/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import '../widgets/pairing_scanner.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/identity_sync.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';

/// Target side of the QR login flow: the device that is not logged in yet scans
/// the QR shown by an already logged-in client, downloads the sealed identity
/// package from the relay, starts the core with that account and imports the
/// chat snapshot.
class LinkDeviceScreen extends StatefulWidget {
  const LinkDeviceScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<LinkDeviceScreen> createState() => _LinkDeviceScreenState();
}

class _LinkDeviceScreenState extends State<LinkDeviceScreen> {
  bool _busy = false;
  bool _done = false;
  String? _status;
  String? _error;
  String? _lastInvalidCode;

  bool get _scannerSupported => Platform.isAndroid || Platform.isIOS;

  Future<void> _onCode(String codeValue) async {
    if (_busy || _done) return;
    final raw = codeValue.trim();
    if (raw.isEmpty) return;
    final uri = Uri.tryParse(raw);
    if (uri == null ||
        uri.scheme.toLowerCase() != 'fedi3' ||
        uri.host.toLowerCase() != 'pair') {
      return;
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
      return;
    }
    await _link(relayBase: relayBase, code: code);
    return;
  }

  Future<void> _link({
    required String relayBase,
    required String code,
  }) async {
    setState(() {
      _busy = true;
      _error = null;
      _status = null;
    });
    try {
      // The QR appears only after the source uploaded its package, but a scan
      // can still race the upload. Retry briefly before giving up.
      Map<String, dynamic>? sealed;
      for (var attempt = 0; attempt < 5; attempt++) {
        sealed = await IdentitySync.fetchPairingPackage(
          relayBase: relayBase,
          code: code,
        );
        if (sealed != null) break;
        await Future<void>.delayed(const Duration(seconds: 1));
      }
      if (!mounted) return;
      if (sealed == null) {
        setState(() {
          _busy = false;
          _error = context.l10n.linkDeviceExpired;
        });
        return;
      }
      if (!IdentitySync.looksSealed(sealed)) {
        throw const FormatException('invalid pairing envelope');
      }
      final package = await IdentitySync.open(sealed, code);
      final config = IdentitySync.configFromPackage(package);
      // replaceConfig stops a running core first, so this also works when the
      // device already has a local profile and the user logs in again.
      await widget.appState.replaceConfig(config);
      if (!mounted) return;
      if (!widget.appState.isRunning) {
        throw StateError(
            widget.appState.lastError ?? context.l10n.networkErrorTitle);
      }
      // The snapshot is only carried by the pairing package: it is not part of
      // CoreConfig. Importing it is best-effort, login still succeeds.
      final rawSnapshot = package['chat_snapshot'];
      final chatSnapshot =
          rawSnapshot is Map ? rawSnapshot.cast<String, dynamic>() : null;
      if (chatSnapshot != null && chatSnapshot.isNotEmpty) {
        try {
          await CoreApi(
            config: config,
            totalTimeout: const Duration(seconds: 120),
          ).importChatSnapshot(chatSnapshot);
        } catch (e) {
          // Identity login already succeeded; history can be rebuilt from the
          // relay spool later.
          debugPrint('Fedi3 pairing: chat snapshot import failed: $e');
        }
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
                      PairingScanner(
                        enabled: !_busy && !_done,
                        onCode: (value) => unawaited(_onCode(value)),
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
