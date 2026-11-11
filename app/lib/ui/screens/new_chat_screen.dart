/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:material_ui/material_ui.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../theme/ui_tokens.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
import 'chat_thread_screen.dart';

class NewChatScreen extends StatefulWidget {
  const NewChatScreen({super.key, required this.appState});

  final AppState appState;

  @override
  State<NewChatScreen> createState() => _NewChatScreenState();
}

class _NewChatScreenState extends State<NewChatScreen>
    with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 2, vsync: this);
  final TextEditingController _search = TextEditingController();
  final TextEditingController _groupName = TextEditingController();
  final Set<String> _selected = <String>{};

  List<ActorProfile> _contacts = const [];
  final Map<String, int> _scores = {};
  bool _loading = true;
  bool _creating = false;
  String? _error;
  String _query = '';
  String _sort = 'interactions';

  @override
  void initState() {
    super.initState();
    _loadContacts();
  }

  @override
  void dispose() {
    _tabs.dispose();
    _search.dispose();
    _groupName.dispose();
    super.dispose();
  }

  Future<void> _loadContacts() async {
    final cfg = widget.appState.config;
    if (cfg == null || !widget.appState.isRunning) {
      if (mounted) setState(() => _loading = false);
      return;
    }
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final base = cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '');
      final user = Uri.encodeComponent(cfg.username.trim());
      final followersUrl = '$base/users/$user/followers';
      final followingUrl = '$base/users/$user/following';
      final results = await Future.wait([
        ActorRepository.instance.fetchCollectionItems(followersUrl),
        ActorRepository.instance.fetchCollectionItems(followingUrl),
      ]);
      final followers = results[0].toSet();
      final mutual = results[1].where(followers.contains).toList();
      final profiles = <ActorProfile>[];
      for (final id in mutual) {
        try {
          final profile = await ActorRepository.instance.getActor(id);
          if (profile != null) profiles.add(profile);
        } catch (_) {
          // Skip profiles that cannot be fetched; the rest stay selectable.
        }
      }
      final scores = <String, int>{};
      final api = CoreApi(config: cfg);
      try {
        final threads = await api.fetchChatThreads(limit: 100);
        final items = threads['items'];
        if (items is List) {
          for (final raw in items) {
            if (raw is! Map) continue;
            final peers = raw['peer_usernames'];
            if (peers is! List) continue;
            for (final peer in peers) {
              final username = peer.toString();
              for (final p in profiles) {
                if (p.preferredUsername == username) {
                  scores[p.id] = (scores[p.id] ?? 0) + 3;
                }
              }
            }
          }
        }
      } catch (_) {}
      try {
        final notifications = await api.fetchNotifications(limit: 100);
        final items = notifications['items'];
        if (items is List) {
          for (final raw in items) {
            if (raw is! Map) continue;
            final activity = raw['activity'];
            if (activity is! Map) continue;
            final actor = activity['actor']?.toString() ?? '';
            if (actor.isNotEmpty) {
              scores[actor] = (scores[actor] ?? 0) + 1;
            }
          }
        }
      } catch (_) {}
      if (!mounted) return;
      setState(() {
        _contacts = profiles;
        _scores
          ..clear()
          ..addAll(scores);
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.toString();
        _loading = false;
      });
    }
  }

  List<ActorProfile> get _visibleContacts {
    final q = _query.trim().toLowerCase();
    final list = _contacts.where((p) {
      if (q.isEmpty) return true;
      return p.displayName.toLowerCase().contains(q) ||
          p.preferredUsername.toLowerCase().contains(q);
    }).toList();
    if (_sort == 'alpha') {
      list.sort((a, b) {
        final an =
            (a.displayName.trim().isEmpty ? a.preferredUsername : a.displayName)
                .toLowerCase();
        final bn =
            (b.displayName.trim().isEmpty ? b.preferredUsername : b.displayName)
                .toLowerCase();
        return an.compareTo(bn);
      });
    } else {
      list.sort((a, b) {
        final sa = _scores[a.id] ?? 0;
        final sb = _scores[b.id] ?? 0;
        if (sa != sb) return sb.compareTo(sa);
        return a.preferredUsername.compareTo(b.preferredUsername);
      });
    }
    return list;
  }

  Future<void> _openDm(ActorProfile profile) async {
    if (_creating) return;
    setState(() => _creating = true);
    final failedText = context.l10n.chatNewFailed;
    try {
      final cfg = widget.appState.config;
      if (cfg == null) return;
      final api = CoreApi(config: cfg);
      final resp = await api.createChatThread(
        recipients: [profile.id],
        kind: 'dm',
      );
      final threadId = resp['thread_id']?.toString() ?? '';
      if (threadId.isEmpty) {
        throw StateError(failedText);
      }
      if (!mounted) return;
      Navigator.of(context).pushReplacement(
        MaterialPageRoute<void>(
          builder: (_) => ChatThreadScreen(
            appState: widget.appState,
            threadId: threadId,
            title: profile.preferredUsername.trim().isEmpty
                ? profile.displayName
                : '@${profile.preferredUsername.trim().replaceFirst(RegExp(r'^@+'), '')}',
            isGroup: false,
          ),
        ),
      );
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _creating = false);
    }
  }

  Future<void> _createGroup() async {
    if (_creating || _selected.isEmpty) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    setState(() {
      _creating = true;
      _error = null;
    });
    final failedText = context.l10n.chatNewFailed;
    final defaultTitle = context.l10n.chatNewGroupDefault;
    try {
      final api = CoreApi(config: cfg);
      final names = _contacts
          .where((p) => _selected.contains(p.id))
          .map((p) => p.displayName.trim().isEmpty
              ? p.preferredUsername
              : p.displayName.trim())
          .toList();
      final title = _groupName.text.trim().isEmpty
          ? names.take(3).join(', ')
          : _groupName.text.trim();
      final resp = await api.createChatThread(
        recipients: _selected.toList(),
        title: title.isEmpty ? defaultTitle : title,
        kind: 'group',
      );
      final threadId = resp['thread_id']?.toString() ?? '';
      if (threadId.isEmpty) {
        throw StateError(failedText);
      }
      if (!mounted) return;
      Navigator.of(context).pushReplacement(
        MaterialPageRoute<void>(
          builder: (_) => ChatThreadScreen(
            appState: widget.appState,
            threadId: threadId,
            title: title.isEmpty ? defaultTitle : title,
            isGroup: true,
          ),
        ),
      );
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _creating = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config;
    if (cfg == null) {
      return Scaffold(
        appBar: AppBar(title: Text(context.l10n.chatNewTitle)),
        body: const Center(child: CircularProgressIndicator()),
      );
    }
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.chatNewTitle),
        bottom: TabBar(
          controller: _tabs,
          tabs: [
            Tab(text: context.l10n.chatNewDmTab),
            Tab(text: context.l10n.chatNewGroupTab),
          ],
        ),
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _error != null && _contacts.isEmpty
              ? Padding(
                  padding: const EdgeInsets.all(16),
                  child: NetworkErrorCard(
                    message: _error!,
                    onRetry: _loadContacts,
                  ),
                )
              : _contacts.isEmpty
                  ? EmptyState(
                      icon: Icons.people_outline_rounded,
                      title: context.l10n.chatNewMutualEmpty,
                      message: context.l10n.chatNewMutualHint,
                    )
                  : TabBarView(
                      controller: _tabs,
                      children: [
                        _buildDmTab(context),
                        _buildGroupTab(context),
                      ],
                    ),
    );
  }

  Widget _buildSearchHeader(BuildContext context, {Widget? trailing}) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
      child: Row(
        children: [
          Expanded(
            child: TextField(
              controller: _search,
              decoration: InputDecoration(
                hintText: context.l10n.chatNewSearchHint,
                prefixIcon: const Icon(Icons.search_rounded),
                isDense: true,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(UiTokens.radiusInput),
                ),
              ),
              onChanged: (v) => setState(() => _query = v),
            ),
          ),
          if (trailing != null) ...[const SizedBox(width: 8), trailing],
        ],
      ),
    );
  }

  Widget _sortMenu(BuildContext context) {
    return PopupMenuButton<String>(
      tooltip: context.l10n.chatNewSortTitle,
      initialValue: _sort,
      onSelected: (v) => setState(() => _sort = v),
      itemBuilder: (context) => [
        PopupMenuItem(
          value: 'interactions',
          child: Text(context.l10n.chatNewSortInteractions),
        ),
        PopupMenuItem(
          value: 'alpha',
          child: Text(context.l10n.chatNewSortAlpha),
        ),
      ],
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 10),
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(UiTokens.radiusInput),
          border: Border.all(
            color: Theme.of(context).colorScheme.outlineVariant,
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.sort_rounded, size: 18),
            const SizedBox(width: 4),
            Text(
              _sort == 'alpha'
                  ? context.l10n.chatNewSortAlpha
                  : context.l10n.chatNewSortInteractions,
              style: const TextStyle(fontSize: 12),
            ),
          ],
        ),
      ),
    );
  }

  String _handleLabel(ActorProfile profile, int score) {
    final host = Uri.tryParse(profile.id)?.host ?? '';
    final handle = host.isEmpty
        ? '@${profile.preferredUsername}'
        : '@${profile.preferredUsername}@$host';
    if (score <= 0) return handle;
    return '$handle · ${context.l10n.chatNewInteractions(score)}';
  }

  Widget _buildDmTab(BuildContext context) {
    final contacts = _visibleContacts;
    return Column(
      children: [
        _buildSearchHeader(context, trailing: _sortMenu(context)),
        Expanded(
          child: ListView.separated(
            padding: const EdgeInsets.fromLTRB(12, 4, 12, 16),
            itemCount: contacts.length,
            separatorBuilder: (_, __) => const SizedBox(height: 4),
            itemBuilder: (context, index) {
              final profile = contacts[index];
              final score = _scores[profile.id] ?? 0;
              return Card(
                margin: EdgeInsets.zero,
                child: ListTile(
                  onTap: _creating ? null : () => _openDm(profile),
                  leading: StatusAvatar(
                    imageUrl: profile.iconUrl,
                    size: 42,
                    showStatus: true,
                    statusKey: profile.preferredUsername,
                  ),
                  title: Text(
                    profile.displayName.trim().isEmpty
                        ? profile.preferredUsername
                        : profile.displayName,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                  subtitle: Text(
                    _handleLabel(profile, score),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  trailing: _creating
                      ? const SizedBox(
                          width: 18,
                          height: 18,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.chevron_right_rounded),
                ),
              );
            },
          ),
        ),
      ],
    );
  }

  Widget _buildGroupTab(BuildContext context) {
    final contacts = _visibleContacts;
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
          child: TextField(
            controller: _groupName,
            decoration: InputDecoration(
              labelText: context.l10n.chatNewGroupName,
              hintText: context.l10n.chatNewGroupNameHint,
              border: OutlineInputBorder(
                borderRadius: BorderRadius.circular(UiTokens.radiusInput),
              ),
            ),
          ),
        ),
        _buildSearchHeader(context, trailing: _sortMenu(context)),
        Expanded(
          child: ListView.builder(
            padding: const EdgeInsets.fromLTRB(8, 0, 8, 16),
            itemCount: contacts.length,
            itemBuilder: (context, index) {
              final profile = contacts[index];
              final checked = _selected.contains(profile.id);
              return CheckboxListTile(
                value: checked,
                onChanged: _creating
                    ? null
                    : (v) => setState(() {
                          if (v == true) {
                            _selected.add(profile.id);
                          } else {
                            _selected.remove(profile.id);
                          }
                        }),
                secondary: StatusAvatar(
                  imageUrl: profile.iconUrl,
                  size: 38,
                  showStatus: true,
                  statusKey: profile.preferredUsername,
                ),
                title: Text(
                  profile.displayName.trim().isEmpty
                      ? profile.preferredUsername
                      : profile.displayName,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                subtitle: Text(
                  _handleLabel(profile, 0),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              );
            },
          ),
        ),
        SafeArea(
          top: false,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    context.l10n.chatNewSelectedCount(_selected.length),
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
                FilledButton.icon(
                  onPressed:
                      _creating || _selected.isEmpty ? null : _createGroup,
                  icon: _creating
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.group_add_rounded),
                  label: Text(context.l10n.chatNewCreateGroup),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}
