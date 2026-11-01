/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../theme/ui_tokens.dart';
import 'fedi3_ui.dart';

/// Markdown/MFM-oriented editor with a WordPress-style toolbar, live preview
/// and document statistics. The toolbar inserts plain Markdown/MFM markers so
/// the posted content stays compatible with the Fediverse.
class RichComposer extends StatefulWidget {
  const RichComposer({
    super.key,
    required this.controller,
    this.hintText,
    this.onChanged,
    this.onEmoji,
    this.onMedia,
    this.enabled = true,
    this.minHeight = 240,
  });

  final TextEditingController controller;
  final String? hintText;
  final ValueChanged<String>? onChanged;
  final Future<String?> Function()? onEmoji;
  final VoidCallback? onMedia;
  final bool enabled;
  final double minHeight;

  @override
  State<RichComposer> createState() => _RichComposerState();
}

class _RichComposerState extends State<RichComposer> {
  final _focus = FocusNode();
  final _undoStack = <String>[];
  final _redoStack = <String>[];
  String _lastText = '';
  bool _applyingHistory = false;
  bool _preview = false;

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_onText);
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onText);
    _focus.dispose();
        super.dispose();
  }

  void _onText() {
    final text = widget.controller.text;
    if (!_applyingHistory && text != _lastText) {
      _undoStack.add(_lastText);
      if (_undoStack.length > 100) _undoStack.removeAt(0);
      _redoStack.clear();
      _lastText = text;
    }
    if (mounted) setState(() {});
    widget.onChanged?.call(text);
  }

  void _undoAction() {
    if (_undoStack.isEmpty) return;
    final prev = _undoStack.removeLast();
    _redoStack.add(widget.controller.text);
    _applyingHistory = true;
    widget.controller.text = prev;
    widget.controller.selection =
        TextSelection.collapsed(offset: prev.length);
    _lastText = prev;
    _applyingHistory = false;
    setState(() {});
  }

  void _redoAction() {
    if (_redoStack.isEmpty) return;
    final next = _redoStack.removeLast();
    _undoStack.add(widget.controller.text);
    _applyingHistory = true;
    widget.controller.text = next;
    widget.controller.selection =
        TextSelection.collapsed(offset: next.length);
    _lastText = next;
    _applyingHistory = false;
    setState(() {});
  }

  void _replaceSelection(String text, {int? cursorOffset}) {
    final value = widget.controller.value;
    final sel = value.selection;
    final start = sel.isValid ? sel.start : value.text.length;
    final end = sel.isValid ? sel.end : value.text.length;
    final next = value.text.replaceRange(start, end, text);
    widget.controller.value = TextEditingValue(
      text: next,
      selection: TextSelection.collapsed(
        offset: cursorOffset != null
            ? start + cursorOffset
            : start + text.length,
      ),
    );
    _focus.requestFocus();
  }

  void _wrap(String left, String right) {
    final value = widget.controller.value;
    final sel = value.selection;
    if (!sel.isValid) {
      _replaceSelection('$left$right', cursorOffset: left.length);
      return;
    }
    final selected = value.text.substring(sel.start, sel.end);
    _replaceSelection('$left$selected$right',
        cursorOffset: left.length + selected.length + right.length);
  }

  void _prefixLines(String prefix) {
    final value = widget.controller.value;
    final sel = value.selection;
    final text = value.text;
    var start = sel.isValid ? text.lastIndexOf('\n', sel.start) + 1 : 0;
    var end = sel.isValid ? text.indexOf('\n', sel.end) : text.length;
    if (end < 0) end = text.length;
    final block = text.substring(start, end);
    final lines = block.split('\n');
    final next = lines.map((l) => '$prefix$l').join('\n');
    widget.controller.value = TextEditingValue(
      text: text.replaceRange(start, end, next),
      selection: TextSelection.collapsed(offset: start + next.length),
    );
    _focus.requestFocus();
  }

  void _block(String marker) {
    final value = widget.controller.value;
    final sel = value.selection;
    final start = sel.isValid ? sel.start : value.text.length;
    final prefix = start == 0 || value.text[start - 1] == '\n' ? '' : '\n';
    _replaceSelection('$prefix$marker ');
  }

  Future<void> _insertLink() async {
    final url = await showDialog<String>(
      context: context,
      builder: (context) {
        final ctrl = TextEditingController();
        return AlertDialog(
          title: Text(context.l10n.composerInsertLink),
          content: TextField(
            controller: ctrl,
            autofocus: true,
            decoration:
                InputDecoration(labelText: context.l10n.composerLinkUrl),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: Text(context.l10n.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.of(context).pop(ctrl.text.trim()),
              child: Text(context.l10n.composerInsert),
            ),
          ],
        );
      },
    );
    if (url == null || url.isEmpty) return;
    final value = widget.controller.value;
    final sel = value.selection;
    final label = sel.isValid && !sel.isCollapsed
        ? value.text.substring(sel.start, sel.end)
        : url;
    _replaceSelection('[$label]($url)');
  }

  void _applyBlockStyle(String style) {
    final value = widget.controller.value;
    final text = value.text;
    final sel = value.selection;
    var start = sel.isValid ? text.lastIndexOf('\n', sel.start) + 1 : 0;
    var end = sel.isValid ? text.indexOf('\n', sel.end) : text.length;
    if (end < 0) end = text.length;
    var line = text.substring(start, end);
    line = line.replaceFirst(RegExp(r'^(#{1,3}\s+|>\s+|```)'), '');
    final marker = switch (style) {
      'h1' => '# ',
      'h2' => '## ',
      'h3' => '### ',
      'quote' => '> ',
      _ => '',
    };
    final next = '$marker$line';
    widget.controller.value = TextEditingValue(
      text: text.replaceRange(start, end, next),
      selection: TextSelection.collapsed(offset: start + next.length),
    );
    _focus.requestFocus();
  }

  void _applyFont(String font) {
    final token = switch (font) {
      'serif' => 'font.serif',
      'mono' => 'font.monospace',
      _ => 'font.sans-serif',
    };
    _wrap('\$[$token ', ']');
  }

  void _applySize(String size) {
    final token = switch (size) {
      'l' => 'x1.5',
      'xl' => 'x2',
      _ => 'x1.1',
    };
    _wrap('\$[$token ', ']');
  }

  void _applyAlign(String align) {
    _wrap('\$[$align ', ']');
  }

  int get _words {
    final text = widget.controller.text.trim();
    if (text.isEmpty) return 0;
    return text.split(RegExp(r'\s+')).length;
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final chars = widget.controller.text.characters.length;
    final reading = (_words / 200).ceil();

    Widget iconButton(IconData icon, String tooltip, VoidCallback onTap,
        {bool active = false}) {
      return Tooltip(
        message: tooltip,
        child: InkWell(
          borderRadius: BorderRadius.circular(8),
          onTap: widget.enabled ? onTap : null,
          child: Container(
            width: 34,
            height: 34,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(8),
              color: active ? scheme.primary.withAlpha(40) : null,
            ),
            child: Icon(icon, size: 18, color: scheme.onSurface),
          ),
        ),
      );
    }

    Widget divider() => Container(
          width: 1,
          height: 22,
          margin: const EdgeInsets.symmetric(horizontal: 4),
          color: scheme.outlineVariant,
        );

    return GlassPanel(
      padding: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
            decoration: BoxDecoration(
              color: scheme.surfaceContainerHighest.withAlpha(120),
              borderRadius: const BorderRadius.vertical(
                  top: Radius.circular(UiTokens.radiusCard)),
            ),
            child: Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 2,
              runSpacing: 2,
              children: [
                PopupMenuButton<String>(
                  tooltip: context.l10n.composerBlockStyle,
                  onSelected: _applyBlockStyle,
                  itemBuilder: (context) => [
                    PopupMenuItem(
                        value: 'p',
                        child: Text(context.l10n.composerParagraph)),
                    PopupMenuItem(
                        value: 'h1', child: Text(context.l10n.composerH1)),
                    PopupMenuItem(
                        value: 'h2', child: Text(context.l10n.composerH2)),
                    PopupMenuItem(
                        value: 'h3', child: Text(context.l10n.composerH3)),
                    PopupMenuItem(
                        value: 'quote',
                        child: Text(context.l10n.composerQuote)),
                  ],
                  child: _menuChip(context, context.l10n.composerBlockStyle,
                      Icons.format_size_rounded),
                ),
                PopupMenuButton<String>(
                  tooltip: context.l10n.composerFont,
                  onSelected: _applyFont,
                  itemBuilder: (context) => [
                    PopupMenuItem(
                        value: 'sans', child: Text(context.l10n.composerSans)),
                    PopupMenuItem(
                        value: 'serif',
                        child: Text(context.l10n.composerSerif)),
                    PopupMenuItem(
                        value: 'mono', child: Text(context.l10n.composerMono)),
                  ],
                  child: _menuChip(context, context.l10n.composerFont,
                      Icons.font_download_outlined),
                ),
                PopupMenuButton<String>(
                  tooltip: context.l10n.composerSize,
                  onSelected: _applySize,
                  itemBuilder: (context) => [
                    PopupMenuItem(
                        value: 's', child: Text(context.l10n.composerSizeS)),
                    PopupMenuItem(
                        value: 'l', child: Text(context.l10n.composerSizeL)),
                    PopupMenuItem(
                        value: 'xl', child: Text(context.l10n.composerSizeXL)),
                  ],
                  child: _menuChip(context, context.l10n.composerSize,
                      Icons.text_fields_rounded),
                ),
                divider(),
                iconButton(Icons.format_align_left_rounded,
                    context.l10n.composerAlignLeft, () => _applyAlign('left')),
                iconButton(Icons.format_align_center_rounded,
                    context.l10n.composerAlignCenter,
                    () => _applyAlign('center')),
                iconButton(Icons.format_align_right_rounded,
                    context.l10n.composerAlignRight,
                    () => _applyAlign('right')),
                divider(),
                iconButton(Icons.undo_rounded, context.l10n.composerUndo,
                    () => _undoAction()),
                iconButton(Icons.redo_rounded, context.l10n.composerRedo,
                    () => _redoAction()),
                const SizedBox(width: 8),
                iconButton(
                  _preview ? Icons.edit_rounded : Icons.visibility_rounded,
                  _preview
                      ? context.l10n.composerEdit
                      : context.l10n.composerPreview,
                  () => setState(() => _preview = !_preview),
                  active: _preview,
                ),
              ],
            ),
          ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            decoration: BoxDecoration(
              border: Border(
                bottom: BorderSide(color: scheme.outlineVariant, width: 0.6),
              ),
            ),
            child: Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: 2,
              runSpacing: 2,
              children: [
                iconButton(Icons.format_bold_rounded, context.l10n.composerBold,
                    () => _wrap('**', '**')),
                iconButton(Icons.format_italic_rounded,
                    context.l10n.composerItalic, () => _wrap('*', '*')),
                iconButton(Icons.format_strikethrough_rounded,
                    context.l10n.composerStrike, () => _wrap('~~', '~~')),
                iconButton(Icons.code_rounded, context.l10n.composerCode,
                    () => _wrap('`', '`')),
                iconButton(Icons.link_rounded, context.l10n.composerLink,
                    _insertLink),
                divider(),
                iconButton(Icons.format_list_bulleted_rounded,
                    context.l10n.composerBulletList, () => _prefixLines('- ')),
                iconButton(Icons.format_list_numbered_rounded,
                    context.l10n.composerNumberedList,
                    () => _prefixLines('1. ')),
                iconButton(Icons.format_quote_rounded,
                    context.l10n.composerQuote, () => _block('>')),
                iconButton(Icons.horizontal_rule_rounded,
                    context.l10n.composerDivider, () => _block('---')),
                divider(),
                if (widget.onEmoji != null)
                  iconButton(Icons.emoji_emotions_outlined,
                      context.l10n.composerEmoji, () async {
                    final emoji = await widget.onEmoji!();
                    if (emoji != null && emoji.isNotEmpty) {
                      _replaceSelection(emoji);
                    }
                  }),
                if (widget.onMedia != null)
                  iconButton(Icons.perm_media_outlined,
                      context.l10n.composeAddMedia, widget.onMedia!),
              ],
            ),
          ),
          SizedBox(
            height: widget.minHeight,
            child: _preview
                ? _PreviewPane(text: widget.controller.text)
                : TextField(
                      controller: widget.controller,
                      focusNode: _focus,
                      enabled: widget.enabled,
                      minLines: null,
                      maxLines: null,
                      expands: true,
                      textAlignVertical: TextAlignVertical.top,
                      style: Theme.of(context).textTheme.bodyLarge,
                      decoration: InputDecoration(
                        hintText: widget.hintText,
                        border: InputBorder.none,
                        contentPadding: const EdgeInsets.all(16),
                      ),
                      inputFormatters: const [],
                    ),
          ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            decoration: BoxDecoration(
              border: Border(
                top: BorderSide(color: scheme.outlineVariant, width: 0.6),
              ),
            ),
            child: Row(
              children: [
                Text(
                  context.l10n.composerStats(_words, chars, reading),
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: scheme.onSurfaceVariant,
                      ),
                ),
                const Spacer(),
                if (_preview)
                  Text(context.l10n.composerPreviewMode,
                      style: Theme.of(context)
                          .textTheme
                          .labelSmall
                          ?.copyWith(color: scheme.primary)),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _menuChip(BuildContext context, String label, IconData icon) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: scheme.outlineVariant),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 16, color: scheme.onSurfaceVariant),
          const SizedBox(width: 4),
          Text(label, style: Theme.of(context).textTheme.labelSmall),
          const SizedBox(width: 2),
          Icon(Icons.arrow_drop_down_rounded,
              size: 16, color: scheme.onSurfaceVariant),
        ],
      ),
    );
  }
}

class _PreviewPane extends StatelessWidget {
  const _PreviewPane({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    final blocks = _parse(context, text);
    if (blocks.isEmpty) {
      return Center(
        child: Text(
          context.l10n.composerPreviewEmpty,
          style: TextStyle(
              color: Theme.of(context).colorScheme.onSurfaceVariant),
        ),
      );
    }
    return ListView(
      padding: const EdgeInsets.all(16),
      children: blocks,
    );
  }

  List<Widget> _parse(BuildContext context, String raw) {
    final out = <Widget>[];
    final lines = raw.replaceAll('\r\n', '\n').split('\n');
    var i = 0;
    while (i < lines.length) {
      final line = lines[i];
      if (line.trim().isEmpty) {
        i++;
        continue;
      }
      if (line.startsWith('```')) {
        final buf = <String>[];
        i++;
        while (i < lines.length && !lines[i].startsWith('```')) {
          buf.add(lines[i]);
          i++;
        }
        i++;
        out.add(_codeBlock(context, buf.join('\n')));
        continue;
      }
      final heading = RegExp(r'^(#{1,3})\s+(.*)$').firstMatch(line);
      if (heading != null) {
        final level = heading.group(1)!.length;
        out.add(Padding(
          padding: const EdgeInsets.only(bottom: 8, top: 4),
          child: Text(
            heading.group(2)!,
            style: Theme.of(context).textTheme.titleLarge?.copyWith(
                  fontSize: level == 1 ? 26 : (level == 2 ? 21 : 18),
                  fontWeight: FontWeight.w800,
                ),
          ),
        ));
        i++;
        continue;
      }
      if (line.startsWith('>')) {
        final buf = <String>[];
        while (i < lines.length && lines[i].startsWith('>')) {
          buf.add(lines[i].replaceFirst(RegExp(r'^>\s?'), ''));
          i++;
        }
        out.add(Container(
          margin: const EdgeInsets.symmetric(vertical: 6),
          padding: const EdgeInsets.only(left: 12),
          decoration: BoxDecoration(
            border: Border(
              left: BorderSide(
                  color: Theme.of(context).colorScheme.primary, width: 3),
            ),
          ),
          child: Text(buf.join('\n')),
        ));
        continue;
      }
      if (RegExp(r'^-\s+').hasMatch(line)) {
        final items = <String>[];
        while (i < lines.length && RegExp(r'^-\s+').hasMatch(lines[i])) {
          items.add(lines[i].replaceFirst(RegExp(r'^-\s+'), ''));
          i++;
        }
        out.add(Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final item in items)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text('•  '),
                    Expanded(child: _inline(context, item)),
                  ],
                ),
              ),
          ],
        ));
        continue;
      }
      if (RegExp(r'^\d+\.\s+').hasMatch(line)) {
        final items = <String>[];
        while (i < lines.length && RegExp(r'^\d+\.\s+').hasMatch(lines[i])) {
          items.add(lines[i].replaceFirst(RegExp(r'^\d+\.\s+'), ''));
          i++;
        }
        var n = 1;
        out.add(Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final item in items)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('$n.  '),
                    Expanded(child: _inline(context, item)),
                  ],
                ),
              ),
          ],
        ));
        continue;
      }
      if (RegExp(r'^---+$').hasMatch(line.trim())) {
        out.add(const Divider());
        i++;
        continue;
      }
      out.add(Padding(
        padding: const EdgeInsets.only(bottom: 8),
        child: _inline(context, line),
      ));
      i++;
    }
    return out;
  }

  Widget _codeBlock(BuildContext context, String code) {
    return Container(
      margin: const EdgeInsets.symmetric(vertical: 6),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Text(
        code,
        style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
      ),
    );
  }

  Widget _inline(BuildContext context, String text) {
    final spans = <InlineSpan>[];
    final pattern = RegExp(
        r'(\*\*[^*]+\*\*|\*[^*]+\*|~~[^~]+~~|`[^`]+`|\[[^\]]+\]\([^)]+\))');
    var last = 0;
    for (final m in pattern.allMatches(text)) {
      if (m.start > last) {
        spans.add(TextSpan(text: text.substring(last, m.start)));
      }
      final token = m.group(0)!;
      if (token.startsWith('**')) {
        spans.add(TextSpan(
            text: token.substring(2, token.length - 2),
            style: const TextStyle(fontWeight: FontWeight.w800)));
      } else if (token.startsWith('~~')) {
        spans.add(TextSpan(
            text: token.substring(2, token.length - 2),
            style: const TextStyle(decoration: TextDecoration.lineThrough)));
      } else if (token.startsWith('`')) {
        spans.add(TextSpan(
            text: token.substring(1, token.length - 1),
            style: const TextStyle(fontFamily: 'monospace')));
      } else if (token.startsWith('*')) {
        spans.add(TextSpan(
            text: token.substring(1, token.length - 1),
            style: const TextStyle(fontStyle: FontStyle.italic)));
      } else {
        final link = RegExp(r'\[([^\]]+)\]\(([^)]+)\)').firstMatch(token);
        if (link != null) {
          spans.add(TextSpan(
            text: link.group(1),
            style: TextStyle(
              color: Theme.of(context).colorScheme.primary,
              decoration: TextDecoration.underline,
            ),
          ));
        } else {
          spans.add(TextSpan(text: token));
        }
      }
      last = m.end;
    }
    if (last < text.length) {
      spans.add(TextSpan(text: text.substring(last)));
    }
    return Text.rich(TextSpan(children: spans));
  }
}
