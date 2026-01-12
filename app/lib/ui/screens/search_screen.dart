/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/note_models.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/note_card.dart';
import '../widgets/status_avatar.dart';
import 'profile_screen.dart';

enum SearchTab { posts, users, hashtags }

enum SearchSource { all, local, relay }

class SearchScreen extends StatefulWidget {
  const SearchScreen({super.key, required this.appState, this.initialQuery, this.initialTab = SearchTab.posts});

  final AppState appState;
  final String? initialQuery;
  final SearchTab initialTab;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> with SingleTickerProviderStateMixin {
  late final TextEditingController _searchCtrl;
  late final TabController _tabs;
  Timer? _debounce;

  String _query = '';
  SearchTab _activeTab = SearchTab.posts;
  SearchSource _source = SearchSource.all;
  bool _loading = false;
  String? _error;
  List<_SearchNoteItem> _noteItems = const [];
  List<_SearchUserItem> _userItems = const [];
  List<_HashtagItem> _tagItems = const [];
  String? _notesNext;
  String? _usersNext;
  bool _notesLoadingMore = false;
  bool _usersLoadingMore = false;

  @override
  void initState() {
    super.initState();
    _query = widget.initialQuery?.trim() ?? '';
    _activeTab = widget.initialTab;
    _searchCtrl = TextEditingController(text: _query);
    _tabs = TabController(length: 3, vsync: this, initialIndex: _tabIndex(_activeTab));
    _searchCtrl.addListener(_onQueryChanged);
    _tabs.addListener(() {
      if (!_tabs.indexIsChanging) {
        _activeTab = _tabFromIndex(_tabs.index);
        _runSearch();
      }
    });
    WidgetsBinding.instance.addPostFrameCallback((_) => _runSearch());
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _searchCtrl.removeListener(_onQueryChanged);
    _searchCtrl.dispose();
    _tabs.dispose();
    super.dispose();
  }

  int _tabIndex(SearchTab tab) {
    switch (tab) {
      case SearchTab.posts:
        return 0;
      case SearchTab.users:
        return 1;
      case SearchTab.hashtags:
        return 2;
    }
  }

  SearchTab _tabFromIndex(int idx) {
    switch (idx) {
      case 1:
        return SearchTab.users;
      case 2:
        return SearchTab.hashtags;
      default:
        return SearchTab.posts;
    }
  }

  void _onQueryChanged() {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 300), () {
      final next = _searchCtrl.text.trim();
      if (next == _query) return;
      _query = next;
      _runSearch();
    });
  }

  Future<void> _runSearch({bool reset = true}) async {
    if (!mounted) return;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _notesNext = null;
        _usersNext = null;
      }
    });
    try {
      switch (_activeTab) {
        case SearchTab.posts:
          await _searchNotes(reset: reset);
          break;
        case SearchTab.users:
          await _searchUsers(reset: reset);
          break;
        case SearchTab.hashtags:
          await _searchTags();
          break;
      }
    } catch (e) {
      if (mounted) {
        setState(() => _error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _searchNotes({bool reset = true}) async {
    if (_query.isEmpty) {
      setState(() => _noteItems = const []);
      return;
    }
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    final isTag = _query.startsWith('#');
    final tag = isTag ? _query.substring(1).trim() : '';
    final resp = await api.searchNotes(
      query: isTag ? '' : _query,
      tag: tag,
      cursor: reset ? null : _notesNext,
      source: _source.name,
      consistency: _source == SearchSource.all ? 'full' : 'best',
    );
    final items = reset ? <_SearchNoteItem>[] : List.of(_noteItems);
    final raw = resp['items'];
    if (raw is List) {
      for (final it in raw) {
        if (it is! Map) continue;
        final map = it.cast<String, dynamic>();
        final noteItem = TimelineItem.tryFromActivity(map);
        if (noteItem != null) {
          items.add(_SearchNoteItem(noteItem, map));
        }
      }
    }
    final next = resp['next'];
    if (mounted) {
      setState(() {
        _noteItems = items;
        _notesNext = next is String && next.trim().isNotEmpty ? next : null;
      });
    }
  }

  Future<void> _searchUsers({bool reset = true}) async {
    if (_query.isEmpty) {
      setState(() => _userItems = const []);
      return;
    }
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    final resp = await api.searchUsers(
      query: _query,
      cursor: reset ? null : _usersNext,
      source: _source.name,
      consistency: _source == SearchSource.all ? 'full' : 'best',
    );
    final list = reset ? <_SearchUserItem>[] : List.of(_userItems);
    final raw = resp['items'];
    if (raw is List) {
      for (final it in raw) {
        if (it is! Map) continue;
        final map = it.cast<String, dynamic>();
        final profile = ActorProfile.tryParse(map);
        if (profile != null) {
          list.add(_SearchUserItem(profile, map));
        }
      }
    }
    if (_looksLikeHandleOrUrl(_query)) {
      try {
        final url = await api.resolveActorInput(_query);
        final actor = await ActorRepository.instance.getActor(url);
        if (actor != null && list.every((p) => p.profile.id != actor.id)) {
          list.insert(0, _SearchUserItem(actor, const {}));
        }
      } catch (_) {}
    }
    final next = resp['next'];
    if (mounted) {
      setState(() {
        _userItems = list;
        _usersNext = next is String && next.trim().isNotEmpty ? next : null;
      });
    }
  }

  Future<void> _searchTags() async {
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    final resp = await api.searchHashtags(
      query: _query,
      source: _source.name,
      consistency: 'best',
    );
    final list = <_HashtagItem>[];
    final raw = resp['items'];
    if (raw is List) {
      for (final it in raw) {
        if (it is! Map) continue;
        final name = (it['name'] as String?)?.trim() ?? '';
        final count = (it['count'] as num?)?.toInt() ?? 0;
        if (name.isEmpty) continue;
        list.add(_HashtagItem(name, count));
      }
    }
    if (mounted) setState(() => _tagItems = list);
  }

  bool _looksLikeHandleOrUrl(String input) {
    final v = input.trim();
    return v.contains('@') || v.startsWith('http://') || v.startsWith('https://');
  }

  void _openProfile(ActorProfile profile) {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => ProfileScreen(appState: widget.appState, actorUrl: profile.id),
      ),
    );
  }

  void _openHashtag(String tag) {
    final next = tag.trim();
    if (next.isEmpty) return;
    _tabs.animateTo(_tabIndex(SearchTab.posts));
    _searchCtrl.text = '#$next';
    _query = _searchCtrl.text.trim();
    _runSearch();
  }

  Widget _buildQueryHeader(BuildContext context) {
    if (_query.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
      child: Align(
        alignment: Alignment.centerLeft,
        child: RichText(
          text: TextSpan(
            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(180)),
            children: [
              TextSpan(text: '${context.l10n.searchShowingFor} '),
              TextSpan(
                text: _query,
                style: TextStyle(
                  color: Theme.of(context).colorScheme.primary,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  TextSpan _highlightQuery(BuildContext context, String text) {
    final q = _query.trim();
    if (q.isEmpty) return TextSpan(text: text);
    final lower = text.toLowerCase();
    final idx = lower.indexOf(q.toLowerCase());
    if (idx < 0) return TextSpan(text: text);
    return TextSpan(
      children: [
        TextSpan(text: text.substring(0, idx)),
        TextSpan(
          text: text.substring(idx, idx + q.length),
          style: TextStyle(color: Theme.of(context).colorScheme.primary, fontWeight: FontWeight.w700),
        ),
        TextSpan(text: text.substring(idx + q.length)),
      ],
    );
  }

  Widget _sourceBadge(BuildContext context, String? source) {
    final label = source == 'relay' ? context.l10n.searchSourceRelay : context.l10n.searchSourceLocal;
    return Chip(
      label: Text(label),
      visualDensity: VisualDensity.compact,
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.searchTitle),
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
            child: TextField(
              controller: _searchCtrl,
              decoration: InputDecoration(
                hintText: context.l10n.searchHint,
                prefixIcon: const Icon(Icons.search),
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(14)),
                filled: true,
              ),
              textInputAction: TextInputAction.search,
              onSubmitted: (_) => _runSearch(),
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Align(
              alignment: Alignment.centerLeft,
              child: SegmentedButton<SearchSource>(
                showSelectedIcon: false,
                segments: [
                  ButtonSegment<SearchSource>(value: SearchSource.all, label: Text(context.l10n.searchSourceAll)),
                  ButtonSegment<SearchSource>(value: SearchSource.local, label: Text(context.l10n.searchSourceLocal)),
                  ButtonSegment<SearchSource>(value: SearchSource.relay, label: Text(context.l10n.searchSourceRelay)),
                ],
                selected: {_source},
                onSelectionChanged: (next) {
                  final selected = next.isEmpty ? SearchSource.all : next.first;
                  if (selected == _source) return;
                  setState(() => _source = selected);
                  _runSearch();
                },
              ),
            ),
          ),
          TabBar(
            controller: _tabs,
            tabs: [
              Tab(text: context.l10n.searchTabPosts),
              Tab(text: context.l10n.searchTabUsers),
              Tab(text: context.l10n.searchTabHashtags),
            ],
          ),
          Expanded(
            child: _loading
                ? const Center(child: CircularProgressIndicator())
                : _error != null
                    ? Padding(
                        padding: const EdgeInsets.all(16),
                        child: NetworkErrorCard(
                          message: _error,
                          onRetry: _runSearch,
                        ),
                      )
                    : TabBarView(
                        controller: _tabs,
                        children: [
                          _buildNotes(context),
                          _buildUsers(context),
                          _buildHashtags(context),
                        ],
                      ),
          ),
        ],
      ),
    );
  }

  Widget _buildNotes(BuildContext context) {
    if (_query.isEmpty) {
      return Center(child: Text(context.l10n.searchEmpty));
    }
    if (_noteItems.isEmpty) {
      return Center(child: Text(context.l10n.searchNoResults));
    }
    return ListView.builder(
      padding: const EdgeInsets.all(12),
      itemCount: _noteItems.length + (_notesNext != null ? 1 : 0) + 1,
      itemBuilder: (context, index) {
        if (index == 0) {
          return _buildQueryHeader(context);
        }
        final adjusted = index - 1;
        if (adjusted >= _noteItems.length) {
          return Center(
            child: TextButton(
              onPressed: _notesLoadingMore
                  ? null
                  : () async {
                      setState(() => _notesLoadingMore = true);
                      await _searchNotes(reset: false);
                      if (mounted) setState(() => _notesLoadingMore = false);
                    },
              child: Text(context.l10n.listLoadMore),
            ),
          );
        }
        final item = _noteItems[adjusted];
        final source = item.raw['fedi3SearchSource'] as String?;
        return Padding(
          padding: const EdgeInsets.only(bottom: 10),
          child: Stack(
            children: [
              NoteCard(
                appState: widget.appState,
                item: item.item,
                rawActivity: item.raw,
              ),
              if (source != null)
                Positioned(
                  right: 8,
                  top: 8,
                  child: _sourceBadge(context, source),
                ),
            ],
          ),
        );
      },
    );
  }

  Widget _buildUsers(BuildContext context) {
    if (_query.isEmpty) {
      return Center(child: Text(context.l10n.searchEmpty));
    }
    if (_userItems.isEmpty) {
      return Center(child: Text(context.l10n.searchNoResults));
    }
    return ListView.separated(
      padding: const EdgeInsets.all(12),
      itemCount: _userItems.length + (_usersNext != null ? 1 : 0) + 1,
      separatorBuilder: (_, __) => const SizedBox(height: 8),
      itemBuilder: (context, index) {
        if (index == 0) {
          return _buildQueryHeader(context);
        }
        final adjusted = index - 1;
        if (adjusted >= _userItems.length) {
          return Center(
            child: TextButton(
              onPressed: _usersLoadingMore
                  ? null
                  : () async {
                      setState(() => _usersLoadingMore = true);
                      await _searchUsers(reset: false);
                      if (mounted) setState(() => _usersLoadingMore = false);
                    },
              child: Text(context.l10n.listLoadMore),
            ),
          );
        }
        final user = _userItems[adjusted];
        final source = user.raw['fedi3SearchSource'] as String?;
        return ListTile(
          onTap: () => _openProfile(user.profile),
          leading: StatusAvatar(
            imageUrl: user.profile.iconUrl,
            size: 40,
            showStatus: user.profile.isFedi3,
            statusKey: user.profile.statusKey,
          ),
          title: RichText(
            text: _highlightQuery(context, user.profile.displayName),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          subtitle: RichText(
            text: _highlightQuery(context, user.profile.preferredUsername.isNotEmpty ? user.profile.preferredUsername : user.profile.id),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (source != null) _sourceBadge(context, source),
              const SizedBox(width: 6),
              const Icon(Icons.chevron_right),
            ],
          ),
        );
      },
    );
  }

  Widget _buildHashtags(BuildContext context) {
    if (_tagItems.isEmpty) {
      return Center(child: Text(context.l10n.searchNoResults));
    }
    return ListView.separated(
      padding: const EdgeInsets.all(12),
      itemCount: _tagItems.length + 1,
      separatorBuilder: (_, __) => const SizedBox(height: 8),
      itemBuilder: (context, index) {
        if (index == 0) {
          return _buildQueryHeader(context);
        }
        final tag = _tagItems[index - 1];
        return ListTile(
          onTap: () => _openHashtag(tag.name),
          title: RichText(
            text: _highlightQuery(context, '#${tag.name}'),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          subtitle: Text(context.l10n.searchTagCount(tag.count)),
          trailing: const Icon(Icons.chevron_right),
        );
      },
    );
  }
}

class _SearchNoteItem {
  _SearchNoteItem(this.item, this.raw);

  final TimelineItem item;
  final Map<String, dynamic> raw;
}

class _SearchUserItem {
  _SearchUserItem(this.profile, this.raw);

  final ActorProfile profile;
  final Map<String, dynamic> raw;
}

class _HashtagItem {
  _HashtagItem(this.name, this.count);

  final String name;
  final int count;
}
