/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/chat_models.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../utils/time_ago.dart';
import '../widgets/fedi3_ui.dart';
import 'chat_thread_screen.dart';

/// Local, membership-scoped search across every chat stored by the core.
class ChatSearchScreen extends StatefulWidget {
  const ChatSearchScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<ChatSearchScreen> createState() => _ChatSearchScreenState();
}

class _ChatSearchScreenState extends State<ChatSearchScreen> {
  final TextEditingController _controller = TextEditingController();
  Timer? _debounce;
  List<ChatSearchHit> _results = const [];
  bool _loading = false;
  String? _error;
  String _query = '';

  @override
  void dispose() {
    _debounce?.cancel();
    _controller.dispose();
    super.dispose();
  }

  void _onChanged(String value) {
    setState(() => _query = value);
    _debounce?.cancel();
    final q = value.trim();
    if (q.length < 2) {
      setState(() {
        _results = const [];
        _loading = false;
        _error = null;
      });
      return;
    }
    _debounce = Timer(const Duration(milliseconds: 300), () => _search(q));
  }

  Future<void> _search(String query) async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final hits = await CoreApi(config: cfg)
          .searchAllChatMessages(query: query, limit: 100);
      if (!mounted || query != _query.trim()) return;
      setState(() {
        _results = hits;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  String _username(String actor) {
    final uri = Uri.tryParse(actor.trim());
    if (uri == null) return actor;
    for (final seg in uri.pathSegments.reversed) {
      if (seg.isNotEmpty) return seg;
    }
    return actor;
  }

  Widget _hitTile(ChatSearchHit hit) {
    final title = hit.threadTitle?.trim().isNotEmpty == true
        ? hit.threadTitle!.trim()
        : (hit.threadKind == 'dm'
            ? context.l10n.chatDirectMessage
            : context.l10n.chatGroup);
    final text = hit.text.trim().isEmpty
        ? context.l10n.chatMediaFallback
        : hit.text.trim();
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: GlassPanel(
        padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
        onTap: () => Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (context) => ChatThreadScreen(
              appState: widget.appState,
              threadId: hit.threadId,
              title: title,
              isGroup: hit.threadKind == 'group',
              initialSearch: _query.trim(),
            ),
          ),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Container(
              width: 34,
              height: 34,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: Theme.of(context).colorScheme.primary.withAlpha(35),
              ),
              child: Icon(
                hit.threadKind == 'dm'
                    ? Icons.person_outline_rounded
                    : Icons.group_outlined,
                size: 18,
                color: Theme.of(context).colorScheme.primary,
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(fontWeight: FontWeight.w700),
                        ),
                      ),
                      Text(
                        formatTimeAgo(
                          context,
                          DateTime.fromMillisecondsSinceEpoch(hit.createdAtMs),
                        ),
                        style: Theme.of(context).textTheme.labelSmall?.copyWith(
                              color:
                                  Theme.of(context).colorScheme.onSurfaceVariant,
                            ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 2),
                  Text(
                    '@${_username(hit.senderActor)} · $text',
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: Theme.of(context).colorScheme.onSurfaceVariant,
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

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(context.l10n.chatSearchAllTitle)),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 4),
            child: TextField(
              controller: _controller,
              autofocus: true,
              decoration: InputDecoration(
                hintText: context.l10n.chatSearchAllHint,
                prefixIcon: const Icon(Icons.search_rounded),
                suffixIcon: _query.isEmpty
                    ? null
                    : IconButton(
                        onPressed: () {
                          _controller.clear();
                          _onChanged('');
                        },
                        icon: const Icon(Icons.close_rounded),
                      ),
                isDense: true,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(UiTokens.radiusInput),
                ),
              ),
              onChanged: _onChanged,
            ),
          ),
          if (_loading) const LinearProgressIndicator(minHeight: 2),
          Expanded(child: _buildBody()),
        ],
      ),
    );
  }

  Widget _buildBody() {
    if (_error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text(_error!, textAlign: TextAlign.center),
        ),
      );
    }
    if (_query.trim().length < 2) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text(
            context.l10n.chatSearchAllHint,
            textAlign: TextAlign.center,
            style: TextStyle(
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
        ),
      );
    }
    if (_loading && _results.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_results.isEmpty) {
      return Center(child: Text(context.l10n.chatSearchNoResults));
    }
    return ListView.builder(
      padding: const EdgeInsets.fromLTRB(12, 4, 12, 12),
      itemCount: _results.length,
      itemBuilder: (context, index) => _hitTile(_results[index]),
    );
  }
}
