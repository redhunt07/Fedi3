/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
import '../widgets/timeline_activity_card.dart';

class ProfileScreen extends StatefulWidget {
  const ProfileScreen({super.key, required this.appState, required this.actorUrl});

  final AppState appState;
  final String actorUrl;

  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  ActorProfile? _profile;
  String? _error;
  bool _loading = false;
  List<Map<String, dynamic>> _outbox = const [];
  String _followingStatus = 'none';
  bool _followBusy = false;
  int? _followersCount;
  int? _followingCount;
  Timer? _followPoll;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _followPoll?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final p = await ActorRepository.instance.getActor(widget.actorUrl);
      if (!mounted) return;
      setState(() => _profile = p);
      await _refreshFollowingStatus();
      if (p != null) {
        final followers = p.followers;
        final following = p.following;
        if (followers.isNotEmpty) {
          _followersCount = await ActorRepository.instance.fetchCollectionCount(followers);
        }
        if (following.isNotEmpty) {
          _followingCount = await ActorRepository.instance.fetchCollectionCount(following);
        }
        if (!mounted) return;
        setState(() {});
      }
      if (p?.outbox.isNotEmpty ?? false) {
        final items = await ActorRepository.instance.fetchOutbox(p!.outbox, limit: 20);
        if (!mounted) return;
        setState(() => _outbox = items);
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _refreshFollowingStatus() async {
    final p = _profile;
    final cfg = widget.appState.config;
    if (p == null || cfg == null) return;
    try {
      final api = CoreApi(config: cfg);
      final s = await api.fetchFollowingStatus(p.id);
      if (!mounted) return;
      setState(() {
        if (s == 'none' && (_followingStatus == 'pending' || _followingStatus == 'accepted')) {
          return;
        }
        _followingStatus = s;
      });
      if (_followingStatus == 'pending') {
        _ensureFollowPoll();
      } else {
        _stopFollowPoll();
      }
    } catch (_) {
      // best-effort
    }
  }

  @override
  Widget build(BuildContext context) {
    final cfg = widget.appState.config!;
    final api = CoreApi(config: cfg);

    return Scaffold(
      appBar: AppBar(
        title: Text(_profile?.displayName ?? widget.actorUrl),
        actions: [
          IconButton(onPressed: _loading ? null : _load, icon: const Icon(Icons.refresh)),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          if (_error != null)
            NetworkErrorCard(
              message: _error,
              onRetry: _load,
              compact: true,
            ),
          if (_profile != null)
            _ProfileHeader(
              profile: _profile!,
              followingStatus: _followingStatus,
              followBusy: _followBusy,
              onToggleFollow: () => _toggleFollow(api),
              followersCount: _followersCount,
              followingCount: _followingCount,
            ),
          if (_profile == null && !_loading)
            Center(child: Text(context.l10n.listNoItems)),
          if (_loading) const Center(child: Padding(padding: EdgeInsets.all(16), child: CircularProgressIndicator())),
          if (_outbox.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text(context.l10n.timelineTabHome, style: const TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 8),
            for (final a in _outbox)
              TimelineActivityCard(appState: widget.appState, activity: a),
          ],
        ],
      ),
    );
  }

  Future<void> _toggleFollow(CoreApi api) async {
    final p = _profile;
    if (p == null) return;
    if (_followBusy) return;
    setState(() => _followBusy = true);
    try {
      final following = _followingStatus == 'accepted' || _followingStatus == 'pending';
      setState(() => _followingStatus = following ? 'none' : 'pending');
      if (following) {
        await api.unfollow(p.id);
        _stopFollowPoll();
      } else {
        await api.follow(p.id);
        _ensureFollowPoll();
      }
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
      await _refreshFollowingStatus();
      // Some servers accept asynchronously; re-check after a short delay.
      await Future<void>.delayed(const Duration(seconds: 2));
      if (mounted) await _refreshFollowingStatus();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    } finally {
      if (mounted) setState(() => _followBusy = false);
    }
  }

  void _ensureFollowPoll() {
    if (_followPoll != null) return;
    _followPoll = Timer.periodic(const Duration(seconds: 6), (_) => _refreshFollowingStatus());
  }

  void _stopFollowPoll() {
    _followPoll?.cancel();
    _followPoll = null;
  }
}

class _ProfileHeader extends StatelessWidget {
  const _ProfileHeader({
    required this.profile,
    required this.followingStatus,
    required this.followBusy,
    required this.onToggleFollow,
    required this.followersCount,
    required this.followingCount,
  });

  final ActorProfile profile;
  final String followingStatus;
  final bool followBusy;
  final VoidCallback onToggleFollow;
  final int? followersCount;
  final int? followingCount;

  @override
  Widget build(BuildContext context) {
    final following = followingStatus == 'accepted' || followingStatus == 'pending';
    final label = following ? context.l10n.settingsUnfollow : context.l10n.settingsFollow;
    final fields = profile.fields;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (profile.imageUrl.trim().isNotEmpty)
              ClipRRect(
                borderRadius: BorderRadius.circular(12),
                child: AspectRatio(
                  aspectRatio: 3 / 1,
                  child: Image.network(
                    profile.imageUrl,
                    fit: BoxFit.cover,
                    errorBuilder: (_, __, ___) => const SizedBox.shrink(),
                  ),
                ),
              ),
            if (profile.imageUrl.trim().isNotEmpty) const SizedBox(height: 12),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _Avatar(
                  url: profile.iconUrl,
                  size: 52,
                  showStatus: profile.isFedi3,
                  statusKey: profile.statusKey,
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(profile.displayName, style: const TextStyle(fontWeight: FontWeight.w800, fontSize: 16)),
                      const SizedBox(height: 2),
                      Text(
                        profile.preferredUsername.isNotEmpty ? '@${profile.preferredUsername}' : profile.id,
                        style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179)),
                      ),
                      if (profile.url.trim().isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 4),
                          child: Text(
                            profile.url,
                            style: TextStyle(color: Theme.of(context).colorScheme.primary, fontSize: 12),
                          ),
                        ),
                    ],
                  ),
                ),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    FilledButton(
                      onPressed: followBusy ? null : onToggleFollow,
                      child: followBusy
                          ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                          : Text(label),
                    ),
                    const SizedBox(height: 6),
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (followersCount != null) _StatChip(label: context.l10n.profileFollowers, value: followersCount!),
                        if (followersCount != null && followingCount != null) const SizedBox(width: 6),
                        if (followingCount != null) _StatChip(label: context.l10n.profileFollowing, value: followingCount!),
                      ],
                    ),
                  ],
                ),
              ],
            ),
            if (profile.summary.isNotEmpty) ...[
              const SizedBox(height: 10),
              HtmlWidget(profile.summary),
            ],
            if (fields.isNotEmpty) ...[
              const SizedBox(height: 12),
              for (final f in fields)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      SizedBox(
                        width: 120,
                        child: Text(
                          f.name,
                          style: const TextStyle(fontWeight: FontWeight.w700),
                        ),
                      ),
                      Expanded(child: HtmlWidget(f.value)),
                    ],
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }
}

class _StatChip extends StatelessWidget {
  const _StatChip({required this.label, required this.value});

  final String label;
  final int value;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(120)),
      ),
      child: Text(
        '$label $value',
        style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w700),
      ),
    );
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
