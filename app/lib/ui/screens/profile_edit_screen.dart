/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:mime/mime.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';

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
  bool _locked = false;
  bool _saving = false;
  String? _status;

  _MediaKind? _uploadingKind;
  bool _draggingAvatar = false;
  bool _draggingBanner = false;
  String? _avatarPreviewPath;
  String? _bannerPreviewPath;

  @override
  void initState() {
    super.initState();
    _cfg = widget.appState.config!;
    _displayName = TextEditingController(text: _cfg.displayName);
    _summary = TextEditingController(text: _cfg.summary);
    _locked = _cfg.manuallyApprovesFollowers;
  }

  @override
  void dispose() {
    _displayName.dispose();
    _summary.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final busy = _saving || _uploadingKind != null;

    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.profileEditTitle),
        actions: [
          TextButton(
            onPressed: busy ? null : _save,
            child: Text(context.l10n.profileSave),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(UiTokens.padScreen),
        children: [
          if (_status != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                _status!,
                style: TextStyle(
                  color: _status!.startsWith('OK')
                      ? null
                      : Theme.of(context).colorScheme.error,
                ),
              ),
            ),
          _profileHero(context),
          const SizedBox(height: 18),
          _section(context, context.l10n.profileDetailsTitle),
          GlassPanel(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                TextField(
                  controller: _displayName,
                  enabled: !busy,
                  decoration: InputDecoration(
                    labelText: context.l10n.profileDisplayName,
                    prefixIcon: const Icon(Icons.badge_outlined),
                  ),
                  onChanged: (_) => setState(() {}),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _summary,
                  enabled: !busy,
                  maxLines: 6,
                  decoration: InputDecoration(
                    labelText: context.l10n.profileBio,
                    alignLabelWithHint: true,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 8),
          _section(context, context.l10n.privacyTitle),
          GlassPanel(
            child: Material(
              color: Colors.transparent,
              child: SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(context.l10n.privacyLockedAccount),
                subtitle: Text(context.l10n.privacyLockedAccountHint),
                value: _locked,
                onChanged: busy ? null : (v) => setState(() => _locked = v),
              ),
            ),
          ),
          const SizedBox(height: 8),
          _section(context, context.l10n.profileFieldsTitle),
          if (_cfg.profileFields.isEmpty)
            Padding(
              padding: const EdgeInsets.fromLTRB(4, 2, 4, 10),
              child: Text(
                context.l10n.profileFieldAdd,
                style: TextStyle(
                  color: Theme.of(context).colorScheme.onSurfaceVariant,
                  fontSize: 12,
                ),
              ),
            ),
          for (final entry in _cfg.profileFields)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: GlassPanel(
                padding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                onTap: busy ? null : () => _editField(entry),
                child: Row(
                  children: [
                    _fieldIcon(context),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            entry.name.isEmpty
                                ? context.l10n.profileFieldNameEmpty
                                : entry.name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                                fontWeight: FontWeight.w800, fontSize: 13),
                          ),
                          const SizedBox(height: 2),
                          Text(
                            entry.value,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 12,
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ),
                    IconButton(
                      tooltip: context.l10n.profileFieldEdit,
                      icon: const Icon(Icons.edit_outlined, size: 18),
                      onPressed: busy ? null : () => _editField(entry),
                    ),
                    IconButton(
                      tooltip: context.l10n.cancel,
                      icon: const Icon(Icons.delete_outline, size: 18),
                      onPressed: busy
                          ? null
                          : () {
                              setState(() {
                                _cfg = _copyCfg(
                                  profileFields: _cfg.profileFields
                                      .where((f) => f != entry)
                                      .toList(),
                                );
                              });
                            },
                    ),
                  ],
                ),
              ),
            ),
          OutlinedButton.icon(
            onPressed: busy ? null : _addField,
            icon: const Icon(Icons.add),
            label: Text(context.l10n.profileFieldAdd),
          ),
        ],
      ),
    );
  }

  Widget _profileHero(BuildContext context) {
    return GlassPanel(
      padding: EdgeInsets.zero,
      child: SizedBox(
        height: 246,
        child: Stack(
          clipBehavior: Clip.none,
          children: [
            Positioned(
              top: 0,
              left: 0,
              right: 0,
              height: 168,
              child: _bannerField(context),
            ),
            Positioned(
              left: 20,
              bottom: 16,
              child: _avatarField(context),
            ),
            Positioned(
              left: 132,
              right: 20,
              bottom: 22,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    _displayName.text.trim().isEmpty
                        ? _cfg.username
                        : _displayName.text.trim(),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.titleLarge?.copyWith(
                          fontWeight: FontWeight.w800,
                        ),
                  ),
                  const SizedBox(height: 3),
                  Text(
                    '@${_cfg.username}@${_cfg.domain}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                      fontSize: 12,
                    ),
                  ),
                  const SizedBox(height: 6),
                  Text(
                    context.l10n.profileEditHint,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _bannerField(BuildContext context) {
    final uploading = _uploadingKind == _MediaKind.banner;
    return DropTarget(
      onDragEntered: (_) => setState(() => _draggingBanner = true),
      onDragExited: (_) => setState(() => _draggingBanner = false),
      onDragDone: (details) => _onDrop(details, _MediaKind.banner),
      child: GestureDetector(
        onTap: _saving || _uploadingKind != null
            ? null
            : () => _pickAndUpload(_MediaKind.banner),
        child: ClipRRect(
          borderRadius: const BorderRadius.vertical(
            top: Radius.circular(UiTokens.radiusCard),
          ),
          child: Stack(
            fit: StackFit.expand,
            children: [
              _bannerImage(context),
              DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    colors: [
                      Colors.transparent,
                      Theme.of(context).colorScheme.surface.withAlpha(180),
                    ],
                  ),
                ),
              ),
              if (_draggingBanner)
                _dropOverlay(context, context.l10n.profileImageDropHint),
              if (uploading)
                const ColoredBox(
                  color: Colors.black45,
                  child: Center(
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                ),
              Positioned(
                top: 12,
                right: 12,
                child: _imageActions(context, _MediaKind.banner),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _avatarField(BuildContext context) {
    final uploading = _uploadingKind == _MediaKind.avatar;
    return DropTarget(
      onDragEntered: (_) => setState(() => _draggingAvatar = true),
      onDragExited: (_) => setState(() => _draggingAvatar = false),
      onDragDone: (details) => _onDrop(details, _MediaKind.avatar),
      child: GestureDetector(
        onTap: _saving || _uploadingKind != null
            ? null
            : () => _pickAndUpload(_MediaKind.avatar),
        child: SizedBox(
          width: 96,
          height: 96,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              Container(
                width: 96,
                height: 96,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(
                    color: Theme.of(context).colorScheme.primary,
                    width: 3,
                  ),
                  color: Theme.of(context).colorScheme.surfaceContainerHigh,
                  boxShadow: [
                    BoxShadow(
                      color: Colors.black.withAlpha(80),
                      blurRadius: 18,
                      offset: const Offset(0, 8),
                    ),
                  ],
                ),
                child: ClipOval(child: _avatarImage(context)),
              ),
              if (_draggingAvatar)
                Positioned.fill(
                  child: ClipOval(
                    child: _dropOverlay(
                        context, context.l10n.profileImageDropHint),
                  ),
                ),
              if (uploading)
                const Positioned.fill(
                  child: ClipOval(
                    child: ColoredBox(
                      color: Colors.black45,
                      child: Center(
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                    ),
                  ),
                ),
              Positioned(
                right: 0,
                bottom: 0,
                child: _imageActions(context, _MediaKind.avatar),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _imageActions(BuildContext context, _MediaKind kind) {
    final hasImage = kind == _MediaKind.avatar
        ? _cfg.iconUrl.trim().isNotEmpty
        : _cfg.imageUrl.trim().isNotEmpty;
    return _roundAction(
      context,
      icon: hasImage ? Icons.close_rounded : Icons.photo_camera_outlined,
      tooltip: hasImage
          ? context.l10n.profileRemoveImage
          : context.l10n.profilePickFile,
      onPressed: _saving || _uploadingKind != null
          ? null
          : hasImage
              ? () => _removeImage(kind)
              : () => _pickAndUpload(kind),
    );
  }

  Widget _roundAction(
    BuildContext context, {
    required IconData icon,
    required String tooltip,
    required VoidCallback? onPressed,
  }) {
    return Material(
      color: Theme.of(context).colorScheme.surface.withAlpha(220),
      shape: const CircleBorder(),
      child: IconButton(
        tooltip: tooltip,
        visualDensity: VisualDensity.compact,
        iconSize: 18,
        onPressed: onPressed,
        icon: Icon(icon),
      ),
    );
  }

  Widget _dropOverlay(BuildContext context, String text) {
    return ColoredBox(
      color: Theme.of(context).colorScheme.primary.withAlpha(90),
      child: Center(
        child: Padding(
          padding: const EdgeInsets.all(8),
          child: Text(
            text,
            textAlign: TextAlign.center,
            style: const TextStyle(
              color: Colors.white,
              fontWeight: FontWeight.w800,
              fontSize: 12,
            ),
          ),
        ),
      ),
    );
  }

  Widget _bannerImage(BuildContext context) {
    final local = _bannerPreviewPath;
    if (local != null) {
      return Image.file(File(local), fit: BoxFit.cover);
    }
    final url = _cfg.imageUrl.trim();
    if (url.isNotEmpty) {
      return Image.network(
        url,
        fit: BoxFit.cover,
        errorBuilder: (_, __, ___) => _bannerFallback(context),
      );
    }
    return _bannerFallback(context);
  }

  Widget _bannerFallback(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            Theme.of(context).colorScheme.primary.withAlpha(190),
            const Color(UiTokens.brandCyan).withAlpha(170),
          ],
        ),
      ),
      child: Center(
        child: Icon(
          Icons.image_outlined,
          size: 42,
          color: Colors.white.withAlpha(180),
        ),
      ),
    );
  }

  Widget _avatarImage(BuildContext context) {
    final local = _avatarPreviewPath;
    if (local != null) {
      return Image.file(File(local), fit: BoxFit.cover);
    }
    final url = _cfg.iconUrl.trim();
    if (url.isNotEmpty) {
      return Image.network(
        url,
        fit: BoxFit.cover,
        errorBuilder: (_, __, ___) => _avatarFallback(context),
      );
    }
    return _avatarFallback(context);
  }

  Widget _avatarFallback(BuildContext context) {
    final initial = _cfg.username.isEmpty
        ? '?'
        : _cfg.username.substring(0, 1).toUpperCase();
    return DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            Theme.of(context).colorScheme.primary.withAlpha(190),
            const Color(UiTokens.brandCyan).withAlpha(150),
          ],
        ),
      ),
      child: Center(
        child: Text(
          initial,
          style: const TextStyle(
            color: Colors.white,
            fontSize: 34,
            fontWeight: FontWeight.w800,
          ),
        ),
      ),
    );
  }

  Widget _fieldIcon(BuildContext context) {
    return Container(
      width: 34,
      height: 34,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(11),
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            Theme.of(context).colorScheme.primary.withAlpha(220),
            const Color(UiTokens.brandCyan).withAlpha(190),
          ],
        ),
      ),
      child: const Icon(Icons.link_rounded, size: 18, color: Colors.white),
    );
  }

  Widget _section(BuildContext context, String title) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 14, 4, 8),
      child: Text(
        title.toUpperCase(),
        style: Theme.of(context).textTheme.labelSmall?.copyWith(
              letterSpacing: 1.1,
              fontWeight: FontWeight.w800,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
      ),
    );
  }

  Future<void> _onDrop(DropDoneDetails details, _MediaKind kind) async {
    setState(() {
      if (kind == _MediaKind.avatar) {
        _draggingAvatar = false;
      } else {
        _draggingBanner = false;
      }
    });
    final file = _firstImage(details.files);
    if (file == null) {
      setState(() => _status = context.l10n.profileErrUpload('image'));
      return;
    }
    await _uploadImage(File(file.path), kind);
  }

  XFile? _firstImage(List<XFile> files) {
    for (final file in files) {
      final mime = lookupMimeType(file.path);
      if (mime != null && mime.startsWith('image/')) return file;
    }
    return null;
  }

  Future<void> _pickAndUpload(_MediaKind kind) async {
    const groups = <XTypeGroup>[
      XTypeGroup(
        label: 'Images',
        extensions: [
          'png',
          'jpg',
          'jpeg',
          'gif',
          'webp',
          'bmp',
          'tif',
          'tiff',
          'avif',
          'heic',
          'heif'
        ],
      ),
    ];
    try {
      final file = await openFile(acceptedTypeGroups: groups);
      if (file == null) return;
      await _uploadImage(File(file.path), kind);
    } catch (e) {
      setState(() => _status = context.l10n.profileErrUpload(e.toString()));
    }
  }

  Future<void> _uploadImage(File file, _MediaKind kind) async {
    if (!widget.appState.isRunning) {
      setState(() => _status = context.l10n.profileErrCoreNotRunning);
      return;
    }
    if (_uploadingKind != null) return;

    setState(() {
      _uploadingKind = kind;
      _status = null;
      if (kind == _MediaKind.avatar) {
        _avatarPreviewPath = file.path;
      } else {
        _bannerPreviewPath = file.path;
      }
    });

    try {
      final bytes = await file.readAsBytes();
      final name = file.path.split(RegExp(r'[\\/]+')).last;
      final resp = await CoreApi(config: _cfg).uploadMedia(
        bytes: bytes,
        filename: name.isEmpty ? 'upload.bin' : name,
      );
      final url = (resp['url'] as String?)?.trim() ?? '';
      final mediaType = (resp['mediaType'] as String?)?.trim() ??
          (resp['media_type'] as String?)?.trim() ??
          lookupMimeType(name) ??
          '';
      if (url.isEmpty) throw StateError('upload missing url');

      if (!mounted) return;
      setState(() {
        if (kind == _MediaKind.avatar) {
          _cfg = _copyCfg(iconUrl: url, iconMediaType: mediaType);
        } else {
          _cfg = _copyCfg(imageUrl: url, imageMediaType: mediaType);
        }
        _status = context.l10n.profileUploadOk;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        if (kind == _MediaKind.avatar) {
          _avatarPreviewPath = null;
        } else {
          _bannerPreviewPath = null;
        }
        _status = context.l10n.profileErrUpload(e.toString());
      });
    } finally {
      if (mounted) setState(() => _uploadingKind = null);
    }
  }

  void _removeImage(_MediaKind kind) {
    setState(() {
      if (kind == _MediaKind.avatar) {
        _avatarPreviewPath = null;
        _cfg = _copyCfg(iconUrl: '', iconMediaType: '');
      } else {
        _bannerPreviewPath = null;
        _cfg = _copyCfg(imageUrl: '', imageMediaType: '');
      }
    });
  }

  Future<void> _addField() async {
    final name = TextEditingController();
    final value = TextEditingController();
    final ok = await _showFieldDialog(
        nameCtrl: name, valueCtrl: value, title: context.l10n.profileFieldAdd);
    if (!mounted) return;
    if (!ok) return;
    final n = name.text.trim();
    final v = value.text.trim();
    if (n.isEmpty || v.isEmpty) return;
    setState(() {
      _cfg = _copyCfg(profileFields: [
        ..._cfg.profileFields,
        ProfileFieldKV(name: n, value: v)
      ]);
    });
  }

  Future<void> _editField(ProfileFieldKV entry) async {
    final name = TextEditingController(text: entry.name);
    final value = TextEditingController(text: entry.value);
    final ok = await _showFieldDialog(
        nameCtrl: name, valueCtrl: value, title: context.l10n.profileFieldEdit);
    if (!mounted) return;
    if (!ok) return;
    final n = name.text.trim();
    final v = value.text.trim();
    setState(() {
      _cfg = _copyCfg(
        profileFields: _cfg.profileFields
            .map((f) =>
                identical(f, entry) ? ProfileFieldKV(name: n, value: v) : f)
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
                  TextField(
                      controller: nameCtrl,
                      decoration: InputDecoration(
                          labelText: context.l10n.profileFieldName)),
                  const SizedBox(height: 10),
                  TextField(
                    controller: valueCtrl,
                    maxLines: 3,
                    decoration: InputDecoration(
                        labelText: context.l10n.profileFieldValue),
                  ),
                ],
              ),
              actions: [
                TextButton(
                    onPressed: () => Navigator.of(ctx).pop(false),
                    child: Text(context.l10n.cancel)),
                FilledButton(
                    onPressed: () => Navigator.of(ctx).pop(true),
                    child: Text(context.l10n.save)),
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
      p2pEnable: c.p2pEnable,
      p2pRelayReserve: c.p2pRelayReserve,
      p2pBootstrap: c.p2pBootstrap,
      p2pPublicDiscovery: c.p2pPublicDiscovery,
      p2pPeerExchange: c.p2pPeerExchange,
      webrtcEnable: c.webrtcEnable,
      webrtcIceUrls: c.webrtcIceUrls,
      webrtcIceUsername: c.webrtcIceUsername,
      webrtcIceCredential: c.webrtcIceCredential,
      apRelays: c.apRelays,
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
      postDeliveryMode: c.postDeliveryMode,
      p2pCacheTtlSecs: c.p2pCacheTtlSecs,
      previousPublicBaseUrl: c.previousPublicBaseUrl,
      previousRelayToken: c.previousRelayToken,
      anchorActorUrl: c.anchorActorUrl,
    );
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _status = null;
    });
    try {
      final updated = _copyCfg();
      await widget.appState.replaceConfig(updated);
      try {
        await CoreApi(config: updated).refreshProfile();
        final base =
            updated.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '');
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
      if (mounted) setState(() => _saving = false);
    }
  }
}

enum _MediaKind { avatar, banner }
