/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/backup_codec.dart';
import '../../services/cloud_backup_service.dart';
import '../../services/encryption_manager.dart';
import '../../state/app_state.dart';

class BackupScreen extends StatefulWidget {
  const BackupScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<BackupScreen> createState() => _BackupScreenState();
}

class _BackupScreenState extends State<BackupScreen> {
  final _import = TextEditingController();
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
    if (!widget.appState.isRunning) {
      await widget.appState.startCore();
    }
    final api = CoreApi(config: cfg);
    final coreBackup = await api.exportBackup();
    final encryptionKeys = await EncryptionManager().exportKeys();
    final meta = <String, dynamic>{
      'snapshot_kind': 'full_state',
      'contains_chat': true,
      'contains_follow_graph': true,
      'created_at_ms': DateTime.now().millisecondsSinceEpoch,
      'db_b64_len': coreBackup['db_b64']?.toString().length ?? 0,
      'schema_version': coreBackup['version'],
    };
    return BackupCodec.encode(
      config: cfg,
      prefs: widget.appState.prefs,
      coreBackup: coreBackup,
      encryptionKeys: encryptionKeys,
      meta: meta,
    );
  }

  @override
  void dispose() {
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
          Text(context.l10n.backupCloudTitle,
              style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          Text(context.l10n.backupCloudHint),
          const SizedBox(height: 8),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: [
              FilledButton.icon(
                onPressed: _busy ? null : _uploadCloud,
                icon: const Icon(Icons.cloud_upload),
                label: Text(context.l10n.backupCloudUpload),
              ),
              OutlinedButton.icon(
                onPressed: _busy ? null : _downloadCloud,
                icon: const Icon(Icons.cloud_download),
                label: Text(context.l10n.backupCloudDownload),
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
      // file_selector does not support save dialogs on Android/iOS; fallback to clipboard.
      if (Platform.isAndroid || Platform.isIOS) {
        final text = await _buildBackupText();
        await Clipboard.setData(ClipboardData(text: text));
        if (!mounted) return;
        setState(() => _status = context.l10n.backupExportOk);
        return;
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
      setState(() => _status = context.l10n.backupErr(e.toString()));
    } finally {
      setState(() => _busy = false);
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
      setState(() => _status = context.l10n.backupErr(e.toString()));
    } finally {
      setState(() => _busy = false);
    }
  }

  Future<void> _importFromText() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      final bundle = BackupCodec.decode(_import.text);
      final mergedPrefs = bundle.prefs.copyWith(
        relayAdminToken: bundle.prefs.relayAdminToken.isNotEmpty
            ? bundle.prefs.relayAdminToken
            : widget.appState.prefs.relayAdminToken,
        translationAuthKey: bundle.prefs.translationAuthKey.isNotEmpty
            ? bundle.prefs.translationAuthKey
            : widget.appState.prefs.translationAuthKey,
      );
      final currentCfg = widget.appState.config;
      final mergedConfig = bundle.config.copyWith(
        relayToken: bundle.config.relayToken.isNotEmpty
            ? bundle.config.relayToken
            : (currentCfg?.relayToken ?? ''),
        internalToken: bundle.config.internalToken.isNotEmpty
            ? bundle.config.internalToken
            : (currentCfg?.internalToken ?? CoreConfig.randomToken()),
        previousRelayToken: bundle.config.previousRelayToken ?? currentCfg?.previousRelayToken,
      );

      await widget.appState.stopCore();
      await widget.appState.saveConfig(mergedConfig);
      await widget.appState.savePrefs(mergedPrefs);
      await widget.appState.startCore();
      if (bundle.coreBackup != null) {
        final api = CoreApi(config: mergedConfig);
        await api.importBackup(bundle.coreBackup!);
      }
      if (bundle.encryptionKeys != null) {
        await EncryptionManager().importKeys(bundle.encryptionKeys!);
      }
      await widget.appState.stopCore();
      await widget.appState.startCore();

      if (!mounted) return;
      setState(() {
        _import.clear();
        _status =
            '${context.l10n.backupImportOk} (chat/follow snapshot ripristinato)';
      });
    } catch (e) {
      setState(() => _status = context.l10n.backupErr(e.toString()));
    } finally {
      setState(() => _busy = false);
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
      final bundle = BackupCodec.decode(raw);
      final mergedPrefs = bundle.prefs.copyWith(
        relayAdminToken: bundle.prefs.relayAdminToken.isNotEmpty
            ? bundle.prefs.relayAdminToken
            : widget.appState.prefs.relayAdminToken,
        translationAuthKey: bundle.prefs.translationAuthKey.isNotEmpty
            ? bundle.prefs.translationAuthKey
            : widget.appState.prefs.translationAuthKey,
      );
      final currentCfg = widget.appState.config;
      final mergedConfig = bundle.config.copyWith(
        relayToken: bundle.config.relayToken.isNotEmpty
            ? bundle.config.relayToken
            : (currentCfg?.relayToken ?? ''),
        internalToken: bundle.config.internalToken.isNotEmpty
            ? bundle.config.internalToken
            : (currentCfg?.internalToken ?? CoreConfig.randomToken()),
        previousRelayToken: bundle.config.previousRelayToken ?? currentCfg?.previousRelayToken,
      );

      await widget.appState.stopCore();
      await widget.appState.saveConfig(mergedConfig);
      await widget.appState.savePrefs(mergedPrefs);
      await widget.appState.startCore();
      if (bundle.coreBackup != null) {
        final api = CoreApi(config: mergedConfig);
        await api.importBackup(bundle.coreBackup!);
      }
      if (bundle.encryptionKeys != null) {
        await EncryptionManager().importKeys(bundle.encryptionKeys!);
      }
      await widget.appState.stopCore();
      await widget.appState.startCore();

      if (!mounted) return;
      setState(() {
        _import.clear();
        _status =
            '${context.l10n.backupImportOk} (chat/follow snapshot ripristinato)';
      });
    } catch (e) {
      setState(() => _status = context.l10n.backupErr(e.toString()));
    } finally {
      setState(() => _busy = false);
    }
  }

  Future<void> _uploadCloud() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      final cfg = widget.appState.config;
      if (cfg == null) throw StateError('missing config');
      if (!widget.appState.isRunning) {
        await widget.appState.startCore();
      }
      final service = CloudBackupService(config: cfg);
      await service.upload(prefs: widget.appState.prefs);
      if (!mounted) return;
      setState(() => _status = context.l10n.backupCloudUploadOk);
    } catch (e) {
      setState(() => _status = context.l10n.backupErr(e.toString()));
    } finally {
      setState(() => _busy = false);
    }
  }

  Future<void> _downloadCloud() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      final cfg = widget.appState.config;
      if (cfg == null) throw StateError('missing config');
      final service = CloudBackupService(config: cfg);
      final pkg = await service.download();
      final mergedPrefs = pkg.prefs.copyWith(
        relayAdminToken: widget.appState.prefs.relayAdminToken,
        translationAuthKey: widget.appState.prefs.translationAuthKey,
      );
      final mergedConfig = pkg.config.copyWith(
        relayToken: cfg.relayToken,
        internalToken: cfg.internalToken,
        previousRelayToken: pkg.config.previousRelayToken ?? cfg.previousRelayToken,
      );

      await widget.appState.stopCore();
      await widget.appState.saveConfig(mergedConfig);
      await widget.appState.savePrefs(mergedPrefs);
      await widget.appState.startCore();

      final restoreService = CloudBackupService(config: mergedConfig);
      await restoreService.restore(pkg);
      await widget.appState.stopCore();
      await widget.appState.startCore();

      if (!mounted) return;
      setState(() => _status = context.l10n.backupCloudDownloadOk);
    } catch (e) {
      setState(() => _status = context.l10n.backupErr(e.toString()));
    } finally {
      setState(() => _busy = false);
    }
  }
}
