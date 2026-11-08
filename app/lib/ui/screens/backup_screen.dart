/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/backup_codec.dart';
import '../../state/app_state.dart';

class BackupScreen extends StatefulWidget {
  const BackupScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<BackupScreen> createState() => _BackupScreenState();
}

class _BackupScreenState extends State<BackupScreen> {
  final _import = TextEditingController();
  final _passphrase = TextEditingController();
  bool _busy = false;
  String? _status;

  static const XTypeGroup _jsonTypeGroup = XTypeGroup(
    label: 'JSON',
    extensions: ['json'],
    mimeTypes: ['application/json'],
    uniformTypeIdentifiers: ['public.json'],
  );

  Future<String> _buildBackupText() async {
    final cfg = widget.appState.config;
    if (cfg == null) throw StateError('missing config');
    return BackupCodec.encodeEncrypted(
        config: cfg,
        prefs: widget.appState.prefs,
        passphrase: _passphrase.text);
  }

  @override
  void dispose() {
    _passphrase.dispose();
    _import.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.backupTitle)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(context.l10n.backupScopeWarning),
          const SizedBox(height: 12),
          TextField(
              controller: _passphrase,
              obscureText: true,
              enableSuggestions: false,
              autocorrect: false,
              decoration:
                  InputDecoration(labelText: context.l10n.backupPassphrase)),
          const SizedBox(height: 16),
          if (_status != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                _status!,
                style: TextStyle(
                    color: _status!.startsWith('OK')
                        ? null
                        : Theme.of(context).colorScheme.error),
              ),
            ),
          Text(context.l10n.backupExportTitle,
              style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          Text(context.l10n.backupExportHint),
          const SizedBox(height: 8),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              FilledButton.icon(
                onPressed: _busy ? null : _exportToFile,
                icon: const Icon(Icons.save),
                label: Text(context.l10n.backupExportSave),
              ),
              OutlinedButton.icon(
                onPressed: _busy ? null : _exportToClipboard,
                icon: const Icon(Icons.copy),
                label: Text(context.l10n.backupExportCopy),
              ),
            ],
          ),
          const SizedBox(height: 24),
          Text(context.l10n.backupImportTitle,
              style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          TextField(
            controller: _import,
            maxLines: 10,
            decoration:
                InputDecoration(hintText: context.l10n.backupImportHint),
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              FilledButton.icon(
                onPressed: _busy ? null : _importFromText,
                icon: const Icon(Icons.upload),
                label: Text(context.l10n.backupImportApply),
              ),
              OutlinedButton.icon(
                onPressed: _busy ? null : _importFromFile,
                icon: const Icon(Icons.upload_file),
                label: Text(context.l10n.backupImportFile),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _exportToFile() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      // file_selector has no save dialog on Android/iOS, so hand the file to the
      // system share sheet (Files, Drive, mail) and keep the clipboard as a
      // fallback when sharing is unavailable.
      if (Platform.isAndroid || Platform.isIOS) {
        final text = await _buildBackupText();
        final cfg = widget.appState.config;
        final name = cfg == null
            ? 'fedi3-backup.json'
            : BackupCodec.suggestedFileName(cfg);
        try {
          final dir = await getTemporaryDirectory();
          final file = File('${dir.path}/$name');
          await file.writeAsString(text);
          await Share.shareXFiles(
            [XFile(file.path, mimeType: 'application/json', name: name)],
          );
          if (!mounted) return;
          setState(() => _status = context.l10n.backupExportSaved);
          return;
        } catch (_) {
          await Clipboard.setData(ClipboardData(text: text));
          if (!mounted) return;
          setState(() => _status = context.l10n.backupExportOk);
          return;
        }
      }

      final cfg = widget.appState.config;
      if (cfg == null) throw StateError('missing config');
      final text = await _buildBackupText();

      final result = await getSaveLocation(
        acceptedTypeGroups: [_jsonTypeGroup],
        suggestedName: BackupCodec.suggestedFileName(cfg),
      );
      if (result == null) return;

      final bytes = Uint8List.fromList(utf8.encode(text));
      final xf = XFile.fromData(bytes,
          mimeType: 'application/json',
          name: BackupCodec.suggestedFileName(cfg));
      await xf.saveTo(result.path);

      if (!mounted) return;
      setState(() => _status = context.l10n.backupExportSaved);
    } catch (e) {
      if (mounted) {
        setState(() => _status = context.l10n.backupErr(e.toString()));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _exportToClipboard() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      final text = await _buildBackupText();
      await Clipboard.setData(ClipboardData(text: text));
      if (!mounted) return;
      setState(() => _status = context.l10n.backupExportOk);
    } catch (e) {
      if (mounted) {
        setState(() => _status = context.l10n.backupErr(e.toString()));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// H-57/D3: apply a backup with rollback. replaceConfig already restores the
  /// previous config and restarts the core if the new one fails to start.
  Future<void> _applyBundle(BackupBundle bundle) async {
    final prevCfg = widget.appState.config;
    final prevPrefs = widget.appState.prefs;
    await widget.appState.replaceConfig(bundle.config);
    try {
      await widget.appState.savePrefs(bundle.prefs);
    } catch (e) {
      await widget.appState.savePrefs(prevPrefs);
      if (prevCfg != null) {
        await widget.appState.replaceConfig(prevCfg);
      }
      rethrow;
    }
  }

  Future<void> _importFromText() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      final bundle = await BackupCodec.decodeEncrypted(_import.text,
          passphrase: _passphrase.text);
      await _applyBundle(bundle);

      if (!mounted) return;
      setState(() {
        _import.clear();
        _status = context.l10n.backupImportOk;
      });
    } catch (e) {
      if (mounted) {
        setState(() => _status = context.l10n.backupErr(e.toString()));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _importFromFile() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      final file = await openFile(acceptedTypeGroups: [_jsonTypeGroup]);
      if (file == null) return;
      final raw = await file.readAsString();
      final bundle =
          await BackupCodec.decodeEncrypted(raw, passphrase: _passphrase.text);
      await _applyBundle(bundle);

      if (!mounted) return;
      setState(() {
        _import.clear();
        _status = context.l10n.backupImportOk;
      });
    } catch (e) {
      if (mounted) {
        setState(() => _status = context.l10n.backupErr(e.toString()));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }
}
