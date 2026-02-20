/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:file_selector/file_selector.dart';
import 'package:mime/mime.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../widgets/status_avatar.dart';

class ProfileEditScreen extends StatefulWidget {
  const ProfileEditScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<ProfileEditScreen> createState() => _ProfileEditScreenState();
}

class _ProfileEditScreenState extends State<ProfileEditScreen> {
  late CoreConfig _cfg;
  late final TextEditingController _displayName;
  late final TextEditingController _summary;
  late final TextEditingController _avatarPath;
  late final TextEditingController _bannerPath;
  bool _locked = false;
  bool _saving = false;
  String? _status;

  @override
  void initState() {
    super.initState();
    _cfg = widget.appState.config!;
    _displayName = TextEditingController(text: _cfg.displayName);
    _summary = TextEditingController(text: _cfg.summary);
    _avatarPath = TextEditingController();
    _bannerPath = TextEditingController();
    _locked = _cfg.manuallyApprovesFollowers;
  }

  @override
  void dispose() {
    _displayName.dispose();
    _summary.dispose();
    _avatarPath.dispose();
    _bannerPath.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final api = CoreApi(config: _cfg);

    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.profileEditTitle),
        actions: [
          TextButton(
            onPressed: _saving ? null : _save,
            child: Text(context.l10n.profileSave),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          if (_status != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                _status!,
                style: TextStyle(color: _status!.startsWith('OK') ? null : Theme.of(context).colorScheme.error),
              ),
            ),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Row(
                children: [
                  StatusAvatar(
                    imageUrl: _cfg.iconUrl,
                    size: 44,
                    showStatus: true,
                    statusKey: _cfg.username,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          _displayName.text.trim().isEmpty ? _cfg.username : _displayName.text.trim(),
                          style: const TextStyle(fontWeight: FontWeight.w800),
                        ),
                        Text(
                          '${_cfg.username}@${_cfg.domain}',
                          style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(160)),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _displayName,
            decoration: InputDecoration(labelText: context.l10n.profileDisplayName),
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _summary,
            maxLines: 6,
            decoration: InputDecoration(labelText: context.l10n.profileBio),
          ),
          const SizedBox(height: 12),
          SwitchListTile(
            title: Text(context.l10n.privacyLockedAccount),
            subtitle: Text(context.l10n.privacyLockedAccountHint),
            value: _locked,
            onChanged: (v) => setState(() => _locked = v),
          ),
          const SizedBox(height: 12),
          Text(context.l10n.profileAvatar, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          _uploadRow(
            controller: _avatarPath,
            hint: context.l10n.profileFilePathHint,
            onPick: _saving ? null : () async => _pickFile(_avatarPath),
            onUpload: _saving ? null : () async => _uploadAndSet(api, kind: _MediaKind.avatar),
          ),
          const SizedBox(height: 12),
          Text(context.l10n.profileBanner, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          _uploadRow(
            controller: _bannerPath,
            hint: context.l10n.profileFilePathHint,
            onPick: _saving ? null : () async => _pickFile(_bannerPath),
            onUpload: _saving ? null : () async => _uploadAndSet(api, kind: _MediaKind.banner),
          ),
          const SizedBox(height: 16),
          Text(context.l10n.profileFieldsTitle, style: const TextStyle(fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          for (final entry in _cfg.profileFields)
            Card(
              child: ListTile(
                title: Text(entry.name.isEmpty ? context.l10n.profileFieldNameEmpty : entry.name),
                subtitle: Text(entry.value),
                trailing: IconButton(
                  icon: const Icon(Icons.delete_outline),
                  onPressed: _saving
                      ? null
                      : () {
                          setState(() {
                            _cfg = _copyCfg(profileFields: _cfg.profileFields.where((f) => f != entry).toList());
                          });
                        },
                ),
                onTap: _saving ? null : () => _editField(entry),
              ),
            ),
          OutlinedButton.icon(
            onPressed: _saving ? null : _addField,
            icon: const Icon(Icons.add),
            label: Text(context.l10n.profileFieldAdd),
          ),
        ],
      ),
    );
  }

  Widget _uploadRow({
    required TextEditingController controller,
    required String hint,
    required Future<void> Function()? onPick,
    required Future<void> Function()? onUpload,
  }) {
    return Row(
      children: [
        Expanded(
          child: TextField(
            controller: controller,
            decoration: InputDecoration(hintText: hint),
            readOnly: true,
          ),
        ),
        const SizedBox(width: 10),
        OutlinedButton(
          onPressed: onPick,
          child: Text(context.l10n.profilePickFile),
        ),
        const SizedBox(width: 10),
        FilledButton(
          onPressed: onUpload,
          child: Text(context.l10n.profileUpload),
        ),
      ],
    );
  }

  Future<void> _pickFile(TextEditingController controller) async {
    const groups = <XTypeGroup>[
      XTypeGroup(
        label: 'Images',
        extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'tif', 'tiff', 'avif', 'heic', 'heif'],
      ),
    ];
    try {
      final file = await openFile(acceptedTypeGroups: groups);
      if (file == null) return;
      controller.text = file.path;
    } catch (e) {
      setState(() => _status = context.l10n.profileErrUpload(e.toString()));
    }
  }

  Future<void> _uploadAndSet(CoreApi api, {required _MediaKind kind}) async {
    if (!widget.appState.isRunning) {
      setState(() => _status = context.l10n.profileErrCoreNotRunning);
      return;
    }
    final path = (kind == _MediaKind.avatar ? _avatarPath.text : _bannerPath.text).trim();
    if (path.isEmpty) return;
    try {
      setState(() {
        _saving = true;
        _status = null;
      });
      final file = File(path);
      final bytes = await file.readAsBytes();
      final name = path.split(RegExp(r'[\\/]+')).last;
      final resp = await api.uploadMedia(bytes: bytes, filename: name.isEmpty ? 'upload.bin' : name);
      final url = (resp['url'] as String?)?.trim() ?? '';
      final mediaType = (resp['media_type'] as String?)?.trim() ?? lookupMimeType(name) ?? '';
      if (url.isEmpty) throw StateError('upload missing url');

      if (kind == _MediaKind.avatar) {
        _avatarPath.clear();
        setState(() => _cfg = _copyCfg(iconUrl: url, iconMediaType: mediaType));
      } else {
        _bannerPath.clear();
        setState(() => _cfg = _copyCfg(imageUrl: url, imageMediaType: mediaType));
      }
      setState(() => _status = context.l10n.profileUploadOk);
    } catch (e) {
      setState(() => _status = context.l10n.profileErrUpload(e.toString()));
    } finally {
      setState(() => _saving = false);
    }
  }

  Future<void> _addField() async {
    final name = TextEditingController();
    final value = TextEditingController();
    final ok = await _showFieldDialog(nameCtrl: name, valueCtrl: value, title: context.l10n.profileFieldAdd);
    if (!mounted) return;
    if (!ok) return;
    final n = name.text.trim();
    final v = value.text.trim();
    if (n.isEmpty || v.isEmpty) return;
    setState(() {
      _cfg = _copyCfg(profileFields: [..._cfg.profileFields, ProfileFieldKV(name: n, value: v)]);
    });
  }

  Future<void> _editField(ProfileFieldKV entry) async {
    final name = TextEditingController(text: entry.name);
    final value = TextEditingController(text: entry.value);
    final ok = await _showFieldDialog(nameCtrl: name, valueCtrl: value, title: context.l10n.profileFieldEdit);
    if (!mounted) return;
    if (!ok) return;
    final n = name.text.trim();
    final v = value.text.trim();
    setState(() {
      _cfg = _copyCfg(
        profileFields: _cfg.profileFields
            .map((f) => identical(f, entry) ? ProfileFieldKV(name: n, value: v) : f)
            .toList(),
      );
    });
  }

  Future<bool> _showFieldDialog({
    required TextEditingController nameCtrl,
    required TextEditingController valueCtrl,
    required String title,
  }) async {
    return (await showDialog<bool>(
          context: context,
          builder: (ctx) {
            return AlertDialog(
              title: Text(title),
              content: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextField(controller: nameCtrl, decoration: InputDecoration(labelText: context.l10n.profileFieldName)),
                  const SizedBox(height: 10),
                  TextField(
                    controller: valueCtrl,
                    maxLines: 3,
                    decoration: InputDecoration(labelText: context.l10n.profileFieldValue),
                  ),
                ],
              ),
              actions: [
                TextButton(onPressed: () => Navigator.of(ctx).pop(false), child: Text(context.l10n.cancel)),
                FilledButton(onPressed: () => Navigator.of(ctx).pop(true), child: Text(context.l10n.save)),
              ],
            );
          },
        )) ??
        false;
  }

  CoreConfig _copyCfg({
    String? iconUrl,
    String? iconMediaType,
    String? imageUrl,
    String? imageMediaType,
    List<ProfileFieldKV>? profileFields,
  }) {
    final c = _cfg;
    return CoreConfig(
      username: c.username,
      domain: c.domain,
      publicBaseUrl: c.publicBaseUrl,
      relayWs: c.relayWs,
      relayToken: c.relayToken,
      bind: c.bind,
      internalToken: c.internalToken,
      apRelays: c.apRelays,
      bootstrapFollowActors: c.bootstrapFollowActors,
      displayName: _displayName.text.trim(),
      summary: _summary.text,
      iconUrl: iconUrl ?? c.iconUrl,
      iconMediaType: iconMediaType ?? c.iconMediaType,
      imageUrl: imageUrl ?? c.imageUrl,
      imageMediaType: imageMediaType ?? c.imageMediaType,
      profileFields: profileFields ?? c.profileFields,
      manuallyApprovesFollowers: _locked,
      blockedDomains: c.blockedDomains,
      blockedActors: c.blockedActors,
      previousPublicBaseUrl: c.previousPublicBaseUrl,
      previousRelayToken: c.previousRelayToken,
      upnpPortRangeStart: c.upnpPortRangeStart,
      upnpPortRangeEnd: c.upnpPortRangeEnd,
      upnpLeaseSecs: c.upnpLeaseSecs,
      upnpTimeoutSecs: c.upnpTimeoutSecs,
    );
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _status = null;
    });
    try {
      final updated = _copyCfg();
      await widget.appState.stopCore();
      await widget.appState.saveConfig(updated);
      await widget.appState.startCore();
      try {
        await CoreApi(config: updated).refreshProfile();
        final base = updated.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '');
        final username = updated.username.trim();
        if (base.isNotEmpty && username.isNotEmpty) {
          await ActorRepository.instance.refreshActor('$base/users/$username');
        }
      } catch (_) {
        // Best-effort: actor update will be retried later by followers.
      }
      if (!mounted) return;
      setState(() => _status = context.l10n.profileSavedOk);
    } catch (e) {
      setState(() => _status = context.l10n.profileErrSave(e.toString()));
    } finally {
      setState(() => _saving = false);
    }
  }
}

enum _MediaKind { avatar, banner }
