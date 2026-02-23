/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../model/core_config.dart';
import '../../services/actor_repository.dart';
import '../../services/core_event_stream.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
import '../widgets/timeline_activity_card.dart';
import '../utils/open_url.dart';

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
  String? _outboxNext;
  bool _outboxLoadingMore = false;
  List<Map<String, dynamic>> _featured = const [];
  String _followingStatus = 'none';
  bool _followBusy = false;
  int? _followersCount;
  int? _followingCount;
  Timer? _followPoll;
  StreamSubscription<CoreEvent>? _profileStreamSub;
  Timer? _profileStreamRetry;
  CoreConfig? _profileStreamConfig;
  StreamSubscription<CoreEvent>? _profileNotifStreamSub;
  Timer? _profileNotifStreamRetry;
  CoreConfig? _profileNotifStreamConfig;
  bool _profileRefreshBusy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _followPoll?.cancel();
    _profileStreamRetry?.cancel();
    _profileStreamSub?.cancel();
    _profileNotifStreamRetry?.cancel();
    _profileNotifStreamSub?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
      _featured = const [];
    });
    try {
      final p = await ActorRepository.instance.getActor(widget.actorUrl);
      if (!mounted) return;
      setState(() => _profile = p);
      await _refreshFollowingStatus();
      _startProfileStreamIfNeeded();
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
        final page = await ActorRepository.instance.fetchOutboxPage(p!.outbox, limit: 20);
        if (!mounted) return;
        setState(() {
          _outbox = page.items.where(_isProfileActivity).toList();
          _outboxNext = page.next;
        });
      }
      if (p?.featured.isNotEmpty ?? false) {
        final items = await ActorRepository.instance.fetchCollectionItems(p!.featured, limit: 6);
        if (!mounted) return;
        final out = <Map<String, dynamic>>[];
        for (final it in items) {
          final activity = _normalizeFeaturedItem(it);
          if (activity != null) out.add(activity);
        }
        setState(() => _featured = out);
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  bool _isLocalProfile() {
    final cfg = widget.appState.config;
    final p = _profile;
    if (cfg == null || p == null) return false;
    final me = '${cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '')}/users/${cfg.username}';
    return p.id == me;
  }

  void _startProfileStreamIfNeeded() {
    if (!_isLocalProfile()) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_profileStreamConfig, cfg) && _profileStreamSub != null) return;
    _profileStreamConfig = cfg;
    _profileStreamRetry?.cancel();
    _profileStreamSub?.cancel();
    _profileStreamSub = CoreEventStream(config: cfg).stream(kind: 'profile').listen((ev) {
      if (ev.activityType != 'featured') return;
      _refreshProfileImmediate();
    }, onError: (_) {
      _profileStreamSub?.cancel();
      _profileStreamSub = null;
      _profileStreamRetry?.cancel();
      _profileStreamRetry = Timer(const Duration(seconds: 2), _startProfileStreamIfNeeded);
    });
    _startProfileNotifStreamIfNeeded();
  }

  void _startProfileNotifStreamIfNeeded() {
    if (!_isLocalProfile()) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_profileNotifStreamConfig, cfg) && _profileNotifStreamSub != null) return;
    _profileNotifStreamConfig = cfg;
    _profileNotifStreamRetry?.cancel();
    _profileNotifStreamSub?.cancel();
    _profileNotifStreamSub = CoreEventStream(config: cfg).stream(kind: 'notification').listen((ev) {
      if (ev.activityType != 'Follow' &&
          ev.activityType != 'Undo' &&
          ev.activityType != 'Accept' &&
          ev.activityType != 'Reject') {
        return;
      }
      _refreshProfileImmediate();
    }, onError: (_) {
      _profileNotifStreamSub?.cancel();
      _profileNotifStreamSub = null;
      _profileNotifStreamRetry?.cancel();
      _profileNotifStreamRetry = Timer(const Duration(seconds: 2), _startProfileNotifStreamIfNeeded);
    });
  }

  Future<void> _refreshProfileImmediate() async {
    if (_profileRefreshBusy) return;
    _profileRefreshBusy = true;
    try {
      final p = _profile;
      if (p == null) return;
      final refreshed = await ActorRepository.instance.refreshActor(p.id);
      if (!mounted) return;
      if (refreshed != null) {
        setState(() => _profile = refreshed);
      }
      final active = refreshed ?? p;
      if (active.followers.isNotEmpty) {
        _followersCount = await ActorRepository.instance.fetchCollectionCount(active.followers);
      }
      if (active.following.isNotEmpty) {
        _followingCount = await ActorRepository.instance.fetchCollectionCount(active.following);
      }
      if (!mounted) return;
      setState(() {});
      if (active.outbox.isNotEmpty) {
        final page = await ActorRepository.instance.fetchOutboxPage(active.outbox, limit: 20);
        if (!mounted) return;
        setState(() {
          _outbox = page.items.where(_isProfileActivity).toList();
          _outboxNext = page.next;
        });
      }
      final featuredUrl = active.featured;
      if (featuredUrl.isNotEmpty) {
        final items = await ActorRepository.instance.fetchCollectionItems(featuredUrl, limit: 6);
        if (!mounted) return;
        final out = <Map<String, dynamic>>[];
        for (final it in items) {
          final activity = _normalizeFeaturedItem(it);
          if (activity != null) out.add(activity);
        }
        setState(() => _featured = out);
      }
    } catch (_) {
      // best-effort
    } finally {
      _profileRefreshBusy = false;
    }
  }

  bool _isProfileActivity(Map<String, dynamic> activity) {
    final type = (activity['type'] as String?)?.trim() ?? '';
    if (type != 'Create') return false;
    final obj = activity['object'];
    if (obj is! Map) return false;
    final objType = (obj['type'] as String?)?.trim() ?? '';
    if (objType != 'Note') return false;
    if (_profile == null) return true;
    final actorUrl = _profile!.id;
    final attributedTo = (obj['attributedTo'] as String?)?.trim() ?? '';
    if (attributedTo.isNotEmpty) {
      return attributedTo == actorUrl;
    }
    final actor = (activity['actor'] as String?)?.trim() ?? '';
    return actor.isEmpty || actor == actorUrl;
  }

  Map<String, dynamic>? _normalizeFeaturedItem(dynamic item) {
    if (item is Map) {
      final m = item.cast<String, dynamic>();
      final type = (m['type'] as String?)?.trim() ?? '';
      if (type == 'Create' || type == 'Announce' || type == 'Update') return m;
      if (type == 'Note' || type == 'Article' || type == 'Question') {
        return {
          'type': 'Create',
          'actor': _profile?.id ?? '',
          'object': m,
        };
      }
      if (m['id'] is String) {
        return {
          'type': 'Create',
          'actor': _profile?.id ?? '',
          'object': m['id'],
        };
      }
    }
    if (item is String) {
      final id = item.trim();
      if (id.isEmpty) return null;
      return {
        'type': 'Create',
        'actor': _profile?.id ?? '',
        'object': id,
      };
    }
    return null;
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
          if (_featured.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text(context.l10n.profileFeatured, style: const TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 8),
            for (final a in _featured)
              TimelineActivityCard(appState: widget.appState, activity: a, elevated: true),
          ],
          if (_outbox.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text(context.l10n.timelineTabHome, style: const TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 8),
            for (final a in _outbox)
              TimelineActivityCard(appState: widget.appState, activity: a),
            const SizedBox(height: 8),
            Center(
              child: _outboxLoadingMore
                  ? const CircularProgressIndicator()
                  : OutlinedButton(
                      onPressed: _outboxNext == null ? null : _loadMoreOutbox,
                      child: Text(_outboxNext == null ? context.l10n.listEnd : context.l10n.listLoadMore),
                    ),
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _loadMoreOutbox() async {
    if (_outboxLoadingMore) return;
    final p = _profile;
    final next = _outboxNext;
    if (p == null || next == null || next.trim().isEmpty) return;
    setState(() => _outboxLoadingMore = true);
    try {
      final page = await ActorRepository.instance.fetchOutboxPage(p.outbox, pageUrl: next, limit: 20);
      if (!mounted) return;
      setState(() {
        _outbox.addAll(page.items.where(_isProfileActivity));
        _outboxNext = page.next;
      });
    } catch (_) {
      // best-effort
    } finally {
      if (mounted) setState(() => _outboxLoadingMore = false);
    }
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
    final isPending = followingStatus == 'pending';
    final isFollowing = followingStatus == 'accepted';
    final label = isPending
        ? context.l10n.profileFollowPending
        : (isFollowing ? context.l10n.settingsUnfollow : context.l10n.settingsFollow);
    final fields = profile.fields;
    final hasBanner = profile.imageUrl.trim().isNotEmpty;
    final acct = profile.preferredUsername.isNotEmpty ? '@${profile.preferredUsername}' : profile.id;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (profile.movedTo.trim().isNotEmpty)
              Container(
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest.withAlpha(120),
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(color: Theme.of(context).colorScheme.outlineVariant.withAlpha(120)),
                ),
                child: InkWell(
                  onTap: () => openUrlExternal(profile.movedTo),
                  child: Row(
                    children: [
                      const Icon(Icons.arrow_forward, size: 18),
                      const SizedBox(width: 8),
                      Expanded(child: Text(context.l10n.profileMovedTo(profile.movedTo))),
                    ],
                  ),
                ),
              ),
            if (profile.movedTo.trim().isNotEmpty) const SizedBox(height: 10),
            ClipRRect(
              borderRadius: BorderRadius.circular(12),
              child: SizedBox(
                height: 200,
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    if (hasBanner)
                      Image.network(
                        profile.imageUrl,
                        fit: BoxFit.cover,
                        errorBuilder: (_, __, ___) => const SizedBox.shrink(),
                      )
                    else
                      Container(color: Theme.of(context).colorScheme.surfaceContainerHighest),
                    DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: LinearGradient(
                          begin: Alignment.topCenter,
                          end: Alignment.bottomCenter,
                          colors: [
                            Colors.black.withAlpha(20),
                            Colors.black.withAlpha(140),
                          ],
                        ),
                      ),
                    ),
                    Positioned(
                      left: 16,
                      right: 16,
                      bottom: 16,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            profile.displayName,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(fontWeight: FontWeight.w800, fontSize: 20),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            acct,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(200)),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 12),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Transform.translate(
                  offset: const Offset(0, -32),
                  child: _Avatar(
                    url: profile.iconUrl,
                    size: 72,
                    showStatus: profile.isFedi3,
                    statusKey: profile.statusKey,
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      if (!hasBanner) ...[
                        Text(profile.displayName, style: const TextStyle(fontWeight: FontWeight.w800, fontSize: 16)),
                        const SizedBox(height: 2),
                        Text(
                          acct,
                          style: TextStyle(color: Theme.of(context).colorScheme.onSurface.withAlpha(179)),
                        ),
                      ],
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
            if (profile.aliases.isNotEmpty) ...[
              const SizedBox(height: 10),
              Text(context.l10n.profileAliases, style: const TextStyle(fontWeight: FontWeight.w600)),
              const SizedBox(height: 6),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final a in profile.aliases)
                    ActionChip(
                      label: Text(a),
                      onPressed: () => openUrlExternal(a),
                    ),
                ],
              ),
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
                      if (_fieldHasVerified(profile, f.value))
                        const Padding(
                          padding: EdgeInsets.only(left: 6, top: 2),
                          child: Icon(Icons.verified, size: 16),
                        ),
                    ],
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }

  bool _fieldHasVerified(ActorProfile profile, String value) {
    if (profile.verifiedLinks.isEmpty) return false;
    final matches = RegExp('href\\s*=\\s*([\'"])([^\'"]+)\\1', caseSensitive: false)
        .allMatches(value)
        .map((m) => m.group(2)?.trim() ?? '')
        .where((v) => v.isNotEmpty);
    for (final href in matches) {
      if (profile.verifiedLinks.contains(href)) {
        return true;
      }
    }
    return false;
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
