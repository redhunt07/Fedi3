/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:ffi' show Abi;
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/client_update_service.dart';
import '../widgets/markdown_view.dart';

String get clientArchitecture => Abi.current().toString().split('_').last;

class ClientUpdatesScreen extends StatefulWidget {
  const ClientUpdatesScreen({super.key, this.service});
  final ClientUpdateService? service;

  @override
  State<ClientUpdatesScreen> createState() => _ClientUpdatesScreenState();
}

class _ClientUpdatesScreenState extends State<ClientUpdatesScreen> {
  late final _service = widget.service ?? ClientUpdateService();
  ClientUpdate? _update;
  VerifiedClientUpdate? _downloaded;
  bool _busy = false;
  bool _checked = false;
  bool _installed = false;
  double? _progress;
  String? _error;

  @override
  void initState() {
    super.initState();
    _check();
  }

  Future<void> _check() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final update = await _service.check(
          os: Platform.operatingSystem, architecture: clientArchitecture);
      if (!mounted) return;
      setState(() {
        _update = update;
        _checked = true;
        _downloaded = null;
        _installed = false;
      });
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _download() async {
    if (_busy || _update == null) return;
    setState(() {
      _busy = true;
      _error = null;
      _progress = 0;
    });
    try {
      final package = await _service
          .download(_update!, await getTemporaryDirectory(),
              onProgress: (received, total) {
        if (mounted) setState(() => _progress = received / total);
      });
      if (mounted) setState(() => _downloaded = package);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) {
        setState(() {
          _busy = false;
          _progress = null;
        });
      }
    }
  }

  Future<void> _install() async {
    final package = _downloaded;
    final target = Platform.environment['APPIMAGE'];
    if (_busy || package == null || target == null) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await _service.installAppImage(package, target);
      if (mounted) setState(() => _installed = true);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _open(Uri uri) async {
    try {
      if (!await launchUrl(uri, mode: LaunchMode.externalApplication)) {
        throw const ClientUpdateException('Cannot open update location');
      }
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = context.l10n;
    final update = _update;
    final package = _downloaded;
    final canInstall = Platform.isLinux &&
        (Platform.environment['APPIMAGE']?.endsWith('.AppImage') ?? false);
    return Scaffold(
      appBar: AppBar(title: Text(l10n.clientUpdatesTitle)),
      body: ListView(padding: const EdgeInsets.all(20), children: [
        Text(l10n.clientUpdatesHint),
        const SizedBox(height: 16),
        OutlinedButton.icon(
            onPressed: _busy ? null : _check,
            icon: const Icon(Icons.refresh),
            label: Text(l10n.clientUpdatesCheck)),
        if (_busy) ...[
          const SizedBox(height: 12),
          LinearProgressIndicator(value: _progress),
        ],
        if (_error != null) ...[
          const SizedBox(height: 12),
          Text(l10n.err(_error!),
              style: TextStyle(color: Theme.of(context).colorScheme.error)),
        ],
        if (_checked && _error == null) ...[
          const SizedBox(height: 16),
          Text(update?.available == true
              ? l10n.clientUpdatesAvailable(update!.version.toString())
              : l10n.clientUpdatesCurrent),
          if (update != null)
            Text(l10n.clientUpdatesVersion(update.currentVersion.toString())),
        ],
        if (update?.available == true) ...[
          const SizedBox(height: 12),
          if (update!.notes.isNotEmpty) MarkdownView(text: update.notes),
          TextButton(
              onPressed: () => _open(update.releaseUrl),
              child: Text(l10n.clientUpdatesRelease)),
          if (update.asset == null) Text(l10n.clientUpdatesManual),
          if (update.asset != null && package == null)
            FilledButton.icon(
                onPressed: _busy ? null : _download,
                icon: const Icon(Icons.download),
                label: Text(l10n.clientUpdatesDownload)),
        ],
        if (package != null) ...[
          const SizedBox(height: 12),
          Text(l10n.clientUpdatesVerified),
          SelectableText(package.file.path),
          if (canInstall && !_installed)
            FilledButton(
                onPressed: _busy ? null : _install,
                child: Text(l10n.clientUpdatesInstall)),
          if (!canInstall) ...[
            Text(l10n.clientUpdatesManual),
            TextButton(
                onPressed: () => _open(package.file.parent.uri),
                child: Text(l10n.clientUpdatesFolder)),
          ],
          if (_installed) Text(l10n.clientUpdatesRestart),
        ],
      ]),
    );
  }
}
