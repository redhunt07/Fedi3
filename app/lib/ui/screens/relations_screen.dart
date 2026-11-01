/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
import 'profile_screen.dart';

/// Lists the actors of a followers/following collection and lets the user
/// follow or unfollow them directly from the list.
class RelationsScreen extends StatefulWidget {
  const RelationsScreen({
    super.key,
    required this.appState,
    required this.actorUrl,
    required this.collectionUrl,
    required this.title,
  });

  final AppState appState;
  final String actorUrl;
  final String collectionUrl;
  final String title;

  @override
  State<RelationsScreen> createState() => _RelationsScreenState();
}

class _RelationsScreenState extends State<RelationsScreen> {
  bool _loading = false;
  String? _error;
  List<String> _actorIds = const [];
  final Map<String, ActorProfile?> _profiles = {};
  final Map<String, String> _status = {};
  final Set<String> _busy = {};

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final ids = await ActorRepository.instance
          .fetchCollectionItems(widget.collectionUrl);
      final profiles = <String, ActorProfile?>{};
      for (final id in ids) {
        profiles[id] = await ActorRepository.instance.getActor(id);
      }
      final cfg = widget.appState.config;
      final status = <String, String>{};
      if (cfg != null) {
        final api = CoreApi(config: cfg);
        for (final id in ids) {
          try {
            status[id] = await api.fetchFollowingStatus(id);
          } catch (_) {
            status[id] = 'none';
          }
        }
      }
      if (!mounted) return;
      setState(() {
        _actorIds = ids;
        _profiles
          ..clear()
          ..addAll(profiles);
        _status
          ..clear()
          ..addAll(status);
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _toggleFollow(String actorUrl) async {
    final cfg = widget.appState.config;
    if (cfg == null || _busy.contains(actorUrl)) return;
    final api = CoreApi(config: cfg);
    setState(() => _busy.add(actorUrl));
    try {
      final current = _status[actorUrl] ?? 'none';
      final following = current == 'accepted' || current == 'pending';
      if (following) {
        await api.unfollow(actorUrl);
      } else {
        await api.follow(actorUrl);
      }
      final next = await api.fetchFollowingStatus(actorUrl);
      if (!mounted) return;
      setState(() => _status[actorUrl] = next);
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    } finally {
      if (mounted) setState(() => _busy.remove(actorUrl));
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.title),
        actions: [
          IconButton(
              onPressed: _loading ? null : _load,
              icon: const Icon(Icons.refresh)),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _error != null
              ? ListView(
                  padding: const EdgeInsets.all(16),
                  children: [
                    NetworkErrorCard(
                        message: _error, onRetry: _load, compact: true),
                  ],
                )
              : _actorIds.isEmpty
                  ? Center(child: Text(context.l10n.listNoItems))
                  : ListView.separated(
                      padding: const EdgeInsets.all(12),
                      itemCount: _actorIds.length,
                      separatorBuilder: (_, __) => const SizedBox(height: 6),
                      itemBuilder: (context, i) {
                        final id = _actorIds[i];
                        final profile = _profiles[id];
                        final handle = _handleFor(profile, id);
                        final status = _status[id] ?? 'none';
                        final following = status == 'accepted' ||
                            status == 'pending';
                        final label = following
                            ? (status == 'pending'
                                ? context.l10n.chatStatusPending
                                : context.l10n.settingsUnfollow)
                            : context.l10n.settingsFollow;
                        final busy = _busy.contains(id);
                        return Card(
                          child: ListTile(
                            leading: StatusAvatar(
                              imageUrl: profile?.iconUrl ?? '',
                              size: 40,
                            ),
                            title: Text(profile?.displayName ?? handle),
                            subtitle: Text(handle),
                            onTap: () => Navigator.of(context).push(
                              MaterialPageRoute<void>(
                                builder: (_) => ProfileScreen(
                                  appState: widget.appState,
                                  actorUrl: id,
                                ),
                              ),
                            ),
                            trailing: FilledButton.tonal(
                              onPressed: busy ? null : () => _toggleFollow(id),
                              child: busy
                                  ? const SizedBox(
                                      width: 14,
                                      height: 14,
                                      child: CircularProgressIndicator(
                                          strokeWidth: 2))
                                  : Text(label),
                            ),
                          ),
                        );
                      },
                    ),
    );
  }

  String _handleFor(ActorProfile? profile, String actorUrl) {
    final host = Uri.tryParse(actorUrl)?.host ?? '';
    final user = profile?.preferredUsername ?? '';
    if (user.isEmpty) return host.isEmpty ? actorUrl : '@$host';
    return host.isEmpty ? '@$user' : '@$user@$host';
  }
}
