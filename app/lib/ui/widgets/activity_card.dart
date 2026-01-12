/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import 'status_avatar.dart';

class ActivityCard extends StatefulWidget {
  const ActivityCard({
    super.key,
    required this.appState,
    required this.activity,
    required this.openActor,
  });

  final AppState appState;
  final Map<String, dynamic> activity;
  final void Function(String actorUrl) openActor;

  @override
  State<ActivityCard> createState() => _ActivityCardState();
}

class _ActivityCardState extends State<ActivityCard> {
  ActorProfile? _actor;
  Map<String, dynamic>? _cachedObject;
  bool _showRaw = false;

  @override
  void initState() {
    super.initState();
    _loadActor();
    _loadCachedObject();
  }

  @override
  void didUpdateWidget(covariant ActivityCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if ((oldWidget.activity['actor'] as String?) != (widget.activity['actor'] as String?)) {
      _actor = null;
      _loadActor();
    }
    if (oldWidget.activity['object'] != widget.activity['object']) {
      _cachedObject = null;
      _loadCachedObject();
    }
  }

  Future<void> _loadActor() async {
    final actorUrl = (widget.activity['actor'] as String?)?.trim() ?? '';
    if (actorUrl.isEmpty) return;
    final p = await ActorRepository.instance.getActor(actorUrl);
    if (!mounted) return;
    setState(() => _actor = p);
  }

  Future<void> _loadCachedObject() async {
    final obj = widget.activity['object'];
    if (obj is! String) return;
    final url = obj.trim();
    if (url.isEmpty) return;
    try {
      final api = CoreApi(config: widget.appState.config!);
      final cached = await api.fetchCachedObject(url);
      if (!mounted) return;
      if (cached == null) return;
      setState(() => _cachedObject = cached);
    } catch (_) {
      // best-effort: ignore cache misses/errors
    }
  }

  @override
  Widget build(BuildContext context) {
    final api = CoreApi(config: widget.appState.config!);
    final canAct = widget.appState.isRunning;

    final type = (widget.activity['type'] as String?)?.trim() ?? '';
    final actorUrl = (widget.activity['actor'] as String?)?.trim() ?? '';
    final rawObj = _cachedObject ?? widget.activity['object'];

    final isBoost = type == 'Announce';
    Map<String, dynamic>? obj;
    if (rawObj is Map) obj = rawObj.cast<String, dynamic>();

    final note = _unwrapNoteFromActivity(type, obj);
    final content = (note?['content'] as String?)?.trim() ?? (note?['name'] as String?)?.trim() ?? '';
    final noteId = (note?['id'] as String?)?.trim() ?? '';
    final noteActor = (note?['attributedTo'] as String?)?.trim() ?? '';
    final published = (note?['published'] as String?)?.trim() ?? (widget.activity['published'] as String?)?.trim() ?? '';
    final replyTo = (note?['inReplyTo'] as String?)?.trim() ?? '';
    final replyPreview = widget.activity['fedi3InReplyToObject'];
    final quotePreview = widget.activity['fedi3QuoteObject'];
    final attachments = _extractAttachments(note);

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _Header(
              actorUrl: actorUrl,
              actor: _actor,
              published: published,
              boosted: isBoost,
              onOpenProfile: actorUrl.isEmpty ? null : () => widget.openActor(actorUrl),
            ),
            const SizedBox(height: 10),
            if (replyPreview is Map || quotePreview is Map)
              _QuotedPreview(
                label: context.l10n.noteInReplyTo,
                inReplyTo: replyTo,
                replyPreview: replyPreview is Map ? replyPreview.cast<String, dynamic>() : null,
                quotePreview: quotePreview is Map ? quotePreview.cast<String, dynamic>() : null,
              ),
            if (content.isNotEmpty)
              HtmlWidget(
                content,
                onTapUrl: (url) {
                  if (_looksLikeActorUrl(url)) {
                    widget.openActor(url);
                    return true;
                  }
                  return false;
                },
              )
            else if (attachments.isEmpty)
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    context.l10n.activityUnsupported,
                    style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179)),
                  ),
                  if (_activitySummary().isNotEmpty) ...[
                    const SizedBox(height: 6),
                    Text(
                      _activitySummary(),
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.onSurface.withAlpha(140),
                        fontSize: 12,
                      ),
                    ),
                  ],
                ],
              ),
            if (attachments.isNotEmpty) ...[
              const SizedBox(height: 10),
              _AttachmentsGrid(attachments: attachments),
            ],
            if (noteId.isNotEmpty) ...[
              const SizedBox(height: 10),
              Row(
                children: [
                  Tooltip(
                    message: context.l10n.activityReply,
                    child: IconButton(
                      onPressed: canAct ? () => _reply(context, api, noteId, noteActor) : null,
                      icon: const Icon(Icons.reply),
                    ),
                  ),
                  Tooltip(
                    message: context.l10n.activityBoost,
                    child: IconButton(
                      onPressed: canAct ? () => _boost(context, api, noteId) : null,
                      icon: const Icon(Icons.repeat),
                    ),
                  ),
                  Tooltip(
                    message: context.l10n.activityLike,
                    child: IconButton(
                      onPressed: (canAct && noteActor.isNotEmpty) ? () => _like(context, api, noteId, noteActor) : null,
                      icon: const Icon(Icons.favorite_border),
                    ),
                  ),
                  const Spacer(),
                  if (!canAct)
                    Text(
                      context.l10n.coreStart,
                      style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
                    ),
                ],
              ),
            ],
            const SizedBox(height: 4),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                onPressed: () => setState(() => _showRaw = !_showRaw),
                child: Text(_showRaw ? context.l10n.activityHideRaw : context.l10n.activityViewRaw),
              ),
            ),
            if (_showRaw)
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Text(
                  const JsonEncoder.withIndent('  ').convert(widget.activity),
                  style: TextStyle(
                    fontFamily: 'monospace',
                    fontSize: 12,
                    color: Theme.of(context).colorScheme.onSurface.withAlpha(204),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }

  Map<String, dynamic>? _unwrapNoteFromActivity(String type, Map<String, dynamic>? obj) {
    if (obj == null) return null;
    if (type == 'Create') {
      final inner = obj['object'];
      if (inner is Map) return inner.cast<String, dynamic>();
      if (obj['type'] == 'Note') return obj;
    }
    if (type == 'Announce') {
      if (obj['type'] == 'Note') return obj;
      final inner = obj['object'];
      if (inner is Map) return inner.cast<String, dynamic>();
    }
    if (obj['type'] == 'Note') return obj;
    return null;
  }

  List<_Attachment> _extractAttachments(Map<String, dynamic>? note) {
    if (note == null) return const [];
    final raw = note['attachment'];
    if (raw is! List) return const [];
    final out = <_Attachment>[];
    for (final it in raw) {
      if (it is String) {
        final s = it.trim();
        if (s.isNotEmpty) out.add(_Attachment(url: s, mediaType: ''));
        continue;
      }
      if (it is! Map) continue;
      final m = it.cast<String, dynamic>();
      var url = '';
      final u = m['url'];
      if (u is String) url = u.trim();
      if (u is Map) url = (u['href'] as String?)?.trim() ?? '';
      final mt = (m['mediaType'] as String?)?.trim() ?? '';
      if (url.isNotEmpty) out.add(_Attachment(url: url, mediaType: mt));
    }
    return out;
  }

  bool _looksLikeActorUrl(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.host.isEmpty) return false;
    return uri.path.startsWith('/users/') || uri.path.startsWith('/@');
  }

  String _activitySummary() {
    final type = (widget.activity['type'] as String?)?.trim() ?? '';
    final actor = widget.activity['actor'];
    final obj = widget.activity['object'];
    final actorLabel = _compactIdentity(actor);
    final objectLabel = _compactIdentity(obj);
    final parts = <String>[];
    if (type.isNotEmpty) parts.add('type=$type');
    if (actorLabel.isNotEmpty) parts.add('actor=$actorLabel');
    if (objectLabel.isNotEmpty) parts.add('object=$objectLabel');
    return parts.join(' | ');
  }

  String _compactIdentity(dynamic value) {
    if (value is String) {
      final v = value.trim();
      if (v.isEmpty) return '';
      final uri = Uri.tryParse(v);
      if (uri == null) return v;
      final segs = uri.pathSegments;
      if (segs.isNotEmpty) return segs.last;
      return uri.host;
    }
    if (value is Map) {
      final map = value.cast<String, dynamic>();
      final id = (map['id'] as String?)?.trim() ?? '';
      final url = (map['url'] as String?)?.trim() ?? '';
      final name = (map['name'] as String?)?.trim() ?? '';
      if (id.isNotEmpty) return _compactIdentity(id);
      if (url.isNotEmpty) return _compactIdentity(url);
      if (name.isNotEmpty) return name;
    }
    return '';
  }

  Future<void> _boost(BuildContext context, CoreApi api, String objectId) async {
    try {
      await api.boost(objectId: objectId, public: true);
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    }
  }

  Future<void> _like(BuildContext context, CoreApi api, String objectId, String objectActor) async {
    try {
      await api.like(objectId: objectId, objectActor: objectActor);
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    }
  }

  Future<void> _reply(BuildContext context, CoreApi api, String inReplyTo, String replyToActor) async {
    final ctrl = TextEditingController();
    try {
      final text = await showDialog<String>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(context.l10n.activityReplyTitle),
          content: TextField(
            controller: ctrl,
            decoration: InputDecoration(hintText: context.l10n.activityReplyHint),
            maxLines: 5,
            minLines: 1,
          ),
          actions: [
            TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(context.l10n.activityCancel)),
            FilledButton(onPressed: () => Navigator.of(context).pop(ctrl.text), child: Text(context.l10n.activitySend)),
          ],
        ),
      );
      final body = text?.trim() ?? '';
      if (body.isEmpty) return;
      await api.postNote(content: body, public: true, mediaIds: const [], inReplyTo: inReplyTo, replyToActor: replyToActor);
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    } finally {
      ctrl.dispose();
    }
  }
}

class _Header extends StatelessWidget {
  const _Header({
    required this.actorUrl,
    required this.actor,
    required this.published,
    required this.boosted,
    required this.onOpenProfile,
  });

  final String actorUrl;
  final ActorProfile? actor;
  final String published;
  final bool boosted;
  final VoidCallback? onOpenProfile;

  @override
  Widget build(BuildContext context) {
    final display = actor?.displayName ?? _fallbackActorLabel(actorUrl);
    final sub = actor?.preferredUsername.isNotEmpty == true ? actor!.preferredUsername : actorUrl;
    final onSurface = Theme.of(context).colorScheme.onSurface;

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _Avatar(
          url: actor?.iconUrl ?? '',
          size: 40,
          showStatus: actor?.isFedi3 == true,
          statusKey: actor?.statusKey,
        ),
        const SizedBox(width: 10),
        Expanded(
          child: InkWell(
            onTap: onOpenProfile,
            borderRadius: BorderRadius.circular(10),
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          display,
                          style: const TextStyle(fontWeight: FontWeight.w800),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      if (boosted) ...[
                        const SizedBox(width: 8),
                        Tooltip(
                          message: context.l10n.activityBoost,
                          child: const Icon(Icons.repeat, size: 18),
                        ),
                      ],
                    ],
                  ),
                  const SizedBox(height: 2),
                  Text(
                    sub,
                    style: TextStyle(color: onSurface.withAlpha(179)),
                    overflow: TextOverflow.ellipsis,
                  ),
                  if (published.isNotEmpty) ...[
                    const SizedBox(height: 2),
                    Text(
                      _formatPublished(published),
                      style: TextStyle(color: onSurface.withAlpha(128), fontSize: 12),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }

  String _formatPublished(String published) {
    final s = published.trim();
    if (s.isEmpty) return '';
    // Keep it simple: show YYYY-MM-DD HH:MM if possible, otherwise raw.
    final dt = DateTime.tryParse(s);
    if (dt == null) return s;
    final local = dt.toLocal();
    String two(int v) => v.toString().padLeft(2, '0');
    return '${local.year}-${two(local.month)}-${two(local.day)} ${two(local.hour)}:${two(local.minute)}';
  }

  String _fallbackActorLabel(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null) return url;
    final host = uri.host;
    if (uri.pathSegments.isNotEmpty && uri.pathSegments.first == 'users' && uri.pathSegments.length >= 2) {
      return '${uri.pathSegments[1]}@$host';
    }
    return host;
  }
}

class _Avatar extends StatelessWidget {
  const _Avatar({
    required this.url,
    required this.size,
    this.showStatus = false,
    this.statusKey,
  });

  final String url;
  final double size;
  final bool showStatus;
  final String? statusKey;

  @override
  Widget build(BuildContext context) {
    return StatusAvatar(
      imageUrl: url,
      size: size,
      showStatus: showStatus,
      statusKey: statusKey,
    );
  }
}

class _Attachment {
  const _Attachment({required this.url, required this.mediaType});
  final String url;
  final String mediaType;
}

class _AttachmentsGrid extends StatelessWidget {
  const _AttachmentsGrid({required this.attachments});

  final List<_Attachment> attachments;

  @override
  Widget build(BuildContext context) {
    final imgs = attachments.where((a) => a.url.startsWith('http')).toList();
    if (imgs.isEmpty) return const SizedBox.shrink();
    final count = imgs.length.clamp(1, 4);
    final display = imgs.take(count).toList();

    return LayoutBuilder(
      builder: (context, c) {
        final w = c.maxWidth;
        const gap = 6.0;
        final cell = (w - gap) / 2;
        return Wrap(
          spacing: gap,
          runSpacing: gap,
          children: [
            for (final a in display)
              ClipRRect(
                borderRadius: BorderRadius.circular(12),
                child: Image.network(
                  a.url,
                  width: count == 1 ? w : cell,
                  height: count == 1 ? w * 0.6 : cell,
                  fit: BoxFit.cover,
                  errorBuilder: (_, __, ___) => Container(
                    width: count == 1 ? w : cell,
                    height: count == 1 ? w * 0.6 : cell,
                    color: Theme.of(context).colorScheme.surfaceContainerHighest,
                    child: const Icon(Icons.broken_image_outlined),
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}

class _QuotedPreview extends StatelessWidget {
  const _QuotedPreview({
    required this.label,
    required this.inReplyTo,
    required this.replyPreview,
    required this.quotePreview,
  });

  final String label;
  final String inReplyTo;
  final Map<String, dynamic>? replyPreview;
  final Map<String, dynamic>? quotePreview;

  @override
  Widget build(BuildContext context) {
    final prev = replyPreview ?? quotePreview;
    if (prev == null) return const SizedBox.shrink();
    final inner = (prev['content'] as String?)?.trim() ?? (prev['name'] as String?)?.trim() ?? '';
    if (inner.isEmpty) return const SizedBox.shrink();

    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Container(
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(130),
          borderRadius: BorderRadius.circular(12),
        ),
        padding: const EdgeInsets.all(10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(label, style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179), fontSize: 12)),
            const SizedBox(height: 6),
            HtmlWidget(inner),
            if (inReplyTo.isNotEmpty) ...[
              const SizedBox(height: 6),
              Text(
                inReplyTo,
                style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(128), fontSize: 12),
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ],
        ),
      ),
    );
  }
}
