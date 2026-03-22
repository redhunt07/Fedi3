/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../widgets/status_avatar.dart';
import 'profile_screen.dart';

enum ProfileConnectionsMode { followers, following }

class ProfileConnectionsScreen extends StatefulWidget {
  const ProfileConnectionsScreen({
    super.key,
    required this.appState,
    required this.collectionUrl,
    required this.mode,
  });

  final AppState appState;
  final String collectionUrl;
  final ProfileConnectionsMode mode;

  @override
  State<ProfileConnectionsScreen> createState() =>
      _ProfileConnectionsScreenState();
}

class _ProfileConnectionsScreenState extends State<ProfileConnectionsScreen> {
  final List<String> _actorUrls = [];
  String? _next;
  bool _loading = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load(reset: true);
  }

  String _title(BuildContext context) {
    return widget.mode == ProfileConnectionsMode.followers
        ? context.l10n.profileFollowers
        : context.l10n.profileFollowing;
  }

  Future<void> _load({required bool reset}) async {
    if (_loading) return;
    final collection = widget.collectionUrl.trim();
    if (collection.isEmpty) return;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _actorUrls.clear();
        _next = null;
      }
    });
    try {
      final page = await ActorRepository.instance.fetchCollectionPage(
        collection,
        pageUrl: reset ? null : _next,
        limit: 40,
      );
      final urls = <String>[];
      for (final item in page.items) {
        final url = _asActorUrl(item);
        if (url.isEmpty) continue;
        urls.add(url);
      }
      if (!mounted) return;
      setState(() {
        final known = _actorUrls.toSet();
        for (final url in urls) {
          if (known.add(url)) {
            _actorUrls.add(url);
          }
        }
        _next = page.next;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  String _asActorUrl(dynamic item) {
    if (item is String) return item.trim();
    if (item is! Map) return '';
    final map = item.cast<String, dynamic>();
    final id = (map['id'] as String?)?.trim() ?? '';
    if (id.isNotEmpty) return id;
    final actor = (map['actor'] as String?)?.trim() ?? '';
    if (actor.isNotEmpty) return actor;
    final object = map['object'];
    if (object is String) return object.trim();
    if (object is Map) {
      return (object['id'] as String?)?.trim() ?? '';
    }
    return '';
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(_title(context))),
      body: ListView(
        padding: const EdgeInsets.all(12),
        children: [
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                _error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          if (_actorUrls.isEmpty && !_loading)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 24),
              child: Center(child: Text(context.l10n.listNoItems)),
            ),
          for (final url in _actorUrls)
            _ActorRow(appState: widget.appState, actorUrl: url),
          const SizedBox(height: 12),
          Center(
            child: _loading
                ? const CircularProgressIndicator()
                : OutlinedButton(
                    onPressed: _next == null ? null : () => _load(reset: false),
                    child: Text(
                      _next == null
                          ? context.l10n.listEnd
                          : context.l10n.listLoadMore,
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _ActorRow extends StatefulWidget {
  const _ActorRow({required this.appState, required this.actorUrl});

  final AppState appState;
  final String actorUrl;

  @override
  State<_ActorRow> createState() => _ActorRowState();
}

class _ActorRowState extends State<_ActorRow> {
  ActorProfile? _profile;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final profile = await ActorRepository.instance.getActor(widget.actorUrl);
    if (!mounted) return;
    setState(() => _profile = profile);
  }

  @override
  Widget build(BuildContext context) {
    final profile = _profile;
    final title = profile?.displayName.trim().isNotEmpty == true
        ? profile!.displayName
        : widget.actorUrl;
    final subtitle = profile?.id ?? widget.actorUrl;
    return Card(
      child: ListTile(
        leading: StatusAvatar(
          imageUrl: profile?.iconUrl ?? '',
          size: 36,
          showStatus: profile?.isFedi3 == true,
          statusKey: profile?.statusKey,
        ),
        title: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis),
        subtitle: Text(subtitle, maxLines: 1, overflow: TextOverflow.ellipsis),
        onTap: () {
          final actor = profile?.id.trim() ?? widget.actorUrl.trim();
          if (actor.isEmpty) return;
          Navigator.of(context).push(
            MaterialPageRoute(
              builder: (_) => ProfileScreen(
                appState: widget.appState,
                actorUrl: actor,
              ),
            ),
          );
        },
      ),
    );
  }
}
