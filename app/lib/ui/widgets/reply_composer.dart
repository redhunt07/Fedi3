/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:typed_data';

import 'package:file_selector/file_selector.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../utils/mfm_codec.dart';

/// Shared by timeline replies and the activity reply dialog. Upload IDs stay
/// with the draft so a failed post can be retried without uploading twice.
class ReplyComposer extends StatefulWidget {
  const ReplyComposer({
    super.key,
    required this.api,
    required this.inReplyTo,
    required this.replyToActor,
    required this.onSent,
    required this.onCancel,
  });

  final CoreApi api;
  final String inReplyTo;
  final String replyToActor;
  final VoidCallback onSent;
  final VoidCallback onCancel;

  @override
  State<ReplyComposer> createState() => _ReplyComposerState();
}

class _ReplyComposerState extends State<ReplyComposer> {
  final _text = TextEditingController();
  final _media = <_ReplyMedia>[];
  bool _sending = false;
  bool _picking = false;
  String? _error;

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _pickMedia() async {
    if (_sending || _picking) return;
    setState(() => _picking = true);
    try {
      final files = await openFiles();
      for (final file in files.take(8 - _media.length)) {
        final bytes = await file.readAsBytes();
        if (!mounted) return;
        setState(() => _media.add(_ReplyMedia(file.name, bytes)));
      }
    } catch (e) {
      if (mounted) {
        setState(
            () => _error = context.l10n.composeErrUnableReadFile(e.toString()));
      }
    } finally {
      if (mounted) setState(() => _picking = false);
    }
  }

  Future<void> _send() async {
    if (_sending || _picking) return;
    final text = _text.text.trim();
    if (text.isEmpty && _media.isEmpty) return;
    setState(() {
      _sending = true;
      _error = null;
    });
    try {
      final mediaIds = <String>[];
      for (final media in _media) {
        if (media.id == null) {
          final response = await widget.api
              .uploadMedia(bytes: media.bytes, filename: media.name);
          final id = response['id']?.toString().trim() ?? '';
          if (id.isEmpty) throw StateError('Media upload returned no ID');
          media.id = id;
        }
        mediaIds.add(media.id!);
      }
      if (!mounted) return;
      await widget.api.postNote(
        content: MfmCodec.toHtml(text),
        public: true,
        mediaIds: mediaIds,
        inReplyTo: widget.inReplyTo,
        replyToActor: widget.replyToActor,
      );
      if (mounted) widget.onSent();
    } catch (e) {
      if (mounted) {
        setState(() => _error = context.l10n.composeErrGeneric(e.toString()));
      }
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final busy = _sending || _picking;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TextField(
          controller: _text,
          enabled: !busy,
          minLines: 1,
          maxLines: 5,
          decoration: InputDecoration(hintText: context.l10n.activityReplyHint),
        ),
        for (final media in _media)
          ListTile(
            dense: true,
            contentPadding: EdgeInsets.zero,
            leading: const Icon(Icons.attach_file),
            title:
                Text(media.name, maxLines: 1, overflow: TextOverflow.ellipsis),
            trailing: IconButton(
              tooltip: context.l10n.replyRemoveAttachment,
              onPressed:
                  busy ? null : () => setState(() => _media.remove(media)),
              icon: const Icon(Icons.close),
            ),
          ),
        if (_error != null)
          Text(_error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error)),
        const SizedBox(height: 8),
        Wrap(
          alignment: WrapAlignment.end,
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: 8,
          runSpacing: 8,
          children: [
            IconButton(
              tooltip: context.l10n.composeAddMedia,
              onPressed: busy || _media.length >= 8 ? null : _pickMedia,
              icon: const Icon(Icons.attach_file),
            ),
            TextButton(
              onPressed: busy ? null : widget.onCancel,
              child: Text(context.l10n.activityCancel),
            ),
            FilledButton(
              onPressed: busy ? null : _send,
              child: _sending
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2))
                  : Text(context.l10n.activitySend),
            ),
          ],
        ),
      ],
    );
  }
}

class _ReplyMedia {
  _ReplyMedia(this.name, this.bytes);

  final String name;
  final Uint8List bytes;
  String? id;
}
