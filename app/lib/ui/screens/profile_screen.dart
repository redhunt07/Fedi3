/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';
import 'package:file_selector/file_selector.dart';

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
import 'profile_connections_screen.dart';

class ProfileScreen extends StatefulWidget {
  const ProfileScreen(
      {super.key, required this.appState, required this.actorUrl});

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
  bool _outboxLoading = false;
  bool _outboxLoaded = false;
  String? _outboxError;
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
  bool _followImportBusy = false;
  String? _followImportJobId;
  Timer? _followImportPoll;
  String? _followImportStatusLabel;
  int? _followingPendingCount;
  int? _followingAcceptedCount;
  String? _activeDataDir;
  String? _activeDid;
  String? _activeUsername;
  Map<String, dynamic>? _followAudit;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _followPoll?.cancel();
    _followImportPoll?.cancel();
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
      _outboxLoading = false;
      _outboxLoaded = false;
      _outboxError = null;
    });
    try {
      final p = await ActorRepository.instance.refreshActor(widget.actorUrl) ??
          await ActorRepository.instance.getActor(widget.actorUrl);
      if (!mounted) return;
      setState(() => _profile = p);
      await _refreshFollowingStatus();
      _startProfileStreamIfNeeded();
      if (p != null) {
        await _refreshProfileCounts(p);
        if (!mounted) return;
        setState(() {});
      }
      if (p?.outbox.isNotEmpty ?? false) {
        await _refreshOutboxOnly(profile: p, silent: true);
      }
      if (p?.featured.isNotEmpty ?? false) {
        final items = await ActorRepository.instance
            .fetchCollectionItems(p!.featured, limit: 6);
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
    final me =
        '${cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '')}/users/${cfg.username}';
    return p.id == me;
  }

  void _startProfileStreamIfNeeded() {
    if (!_isLocalProfile()) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_profileStreamConfig, cfg) && _profileStreamSub != null) {
      return;
    }
    _profileStreamConfig = cfg;
    _profileStreamRetry?.cancel();
    _profileStreamSub?.cancel();
    _profileStreamSub =
        CoreEventStream(config: cfg).stream(kind: 'profile').listen((ev) {
      if (ev.activityType != 'featured') return;
      _refreshProfileImmediate();
    }, onError: (_) {
      _profileStreamSub?.cancel();
      _profileStreamSub = null;
      _profileStreamRetry?.cancel();
      _profileStreamRetry =
          Timer(const Duration(seconds: 2), _startProfileStreamIfNeeded);
    });
    _startProfileNotifStreamIfNeeded();
  }

  void _startProfileNotifStreamIfNeeded() {
    if (!_isLocalProfile()) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_profileNotifStreamConfig, cfg) &&
        _profileNotifStreamSub != null) {
      return;
    }
    _profileNotifStreamConfig = cfg;
    _profileNotifStreamRetry?.cancel();
    _profileNotifStreamSub?.cancel();
    _profileNotifStreamSub =
        CoreEventStream(config: cfg).stream(kind: 'notification').listen((ev) {
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
      _profileNotifStreamRetry =
          Timer(const Duration(seconds: 2), _startProfileNotifStreamIfNeeded);
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
      await _refreshProfileCounts(active);
      if (!mounted) return;
      setState(() {});
      if (active.outbox.isNotEmpty) {
        await _refreshOutboxOnly(profile: active, silent: true);
      }
      final featuredUrl = active.featured;
      if (featuredUrl.isNotEmpty) {
        final items = await ActorRepository.instance
            .fetchCollectionItems(featuredUrl, limit: 6);
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
    if (!_isNoteLikeType(type) &&
        type != 'Create' &&
        type != 'Update' &&
        type != 'Announce') {
      return false;
    }

    if (!_matchesProfileActor(activity)) {
      return false;
    }

    final obj = activity['object'];
    if (obj is String) {
      return obj.trim().isNotEmpty;
    }
    if (obj is! Map) {
      // Some outbox responses can already be note objects.
      return _isNoteLikeType(type);
    }
    final map = obj.cast<String, dynamic>();
    var objType = (map['type'] as String?)?.trim() ?? '';
    if (!_isNoteLikeType(objType) && map['object'] is Map) {
      objType =
          (((map['object'] as Map).cast<String, dynamic>())['type'] as String?)
                  ?.trim() ??
              '';
    }
    if (!_isNoteLikeType(objType)) {
      return false;
    }
    final actorUrl = _profile?.id.trim() ?? '';
    if (actorUrl.isEmpty) {
      return true;
    }
    final attributed = _readActorRefs(map['attributedTo']);
    if (attributed.contains(actorUrl)) {
      return true;
    }
    if (map['object'] is Map) {
      final nested = (map['object'] as Map).cast<String, dynamic>();
      final nestedAttributed = _readActorRefs(nested['attributedTo']);
      if (nestedAttributed.contains(actorUrl)) {
        return true;
      }
    }
    final actor = _readActorRef(activity['actor']);
    return actor.isEmpty || actor == actorUrl;
  }

  bool _isNoteLikeType(String type) {
    final t = type.trim();
    return t == 'Note' || t == 'Article' || t == 'Question' || t == 'Page';
  }

  bool _matchesProfileActor(Map<String, dynamic> activity) {
    final actorUrl = _profile?.id.trim() ?? '';
    if (actorUrl.isEmpty) return true;
    final actor = _readActorRef(activity['actor']);
    if (actor.isNotEmpty && actor == actorUrl) return true;
    final obj = activity['object'];
    if (obj is Map) {
      final map = obj.cast<String, dynamic>();
      final attributed = _readActorRefs(map['attributedTo']);
      if (attributed.contains(actorUrl)) return true;
      final nested = map['object'];
      if (nested is Map) {
        final nestedAttributed =
            _readActorRefs((nested.cast<String, dynamic>())['attributedTo']);
        if (nestedAttributed.contains(actorUrl)) return true;
      }
    }
    return actor.isEmpty;
  }

  String _readActorRef(dynamic raw) {
    if (raw is String) return raw.trim();
    if (raw is Map) {
      final map = raw.cast<String, dynamic>();
      final id = map['id'];
      if (id is String && id.trim().isNotEmpty) return id.trim();
      final url = map['url'];
      if (url is String && url.trim().isNotEmpty) return url.trim();
      if (url is Map) {
        final href = (url['href'] as String?)?.trim() ?? '';
        if (href.isNotEmpty) return href;
      }
      final href = (map['href'] as String?)?.trim() ?? '';
      if (href.isNotEmpty) return href;
      return '';
    }
    if (raw is List) {
      for (final item in raw) {
        final value = _readActorRef(item);
        if (value.isNotEmpty) return value;
      }
    }
    return '';
  }

  Set<String> _readActorRefs(dynamic raw) {
    final out = <String>{};
    if (raw is String) {
      final v = raw.trim();
      if (v.isNotEmpty) out.add(v);
      return out;
    }
    if (raw is Map) {
      final v = _readActorRef(raw);
      if (v.isNotEmpty) out.add(v);
      return out;
    }
    if (raw is List) {
      for (final item in raw) {
        final v = _readActorRef(item);
        if (v.isNotEmpty) out.add(v);
      }
    }
    return out;
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
        if (s == 'none' &&
            (_followingStatus == 'pending' || _followingStatus == 'accepted')) {
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
          IconButton(
              onPressed: _loading ? null : _load,
              icon: const Icon(Icons.refresh)),
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
              followingPendingCount: _followingPendingCount,
              followingAcceptedCount: _followingAcceptedCount,
              activeDataDir: _activeDataDir,
              activeDid: _activeDid,
              activeUsername: _activeUsername,
              followAudit: _followAudit,
              onOpenFollowers: () =>
                  _openConnections(ProfileConnectionsMode.followers),
              onOpenFollowing: () =>
                  _openConnections(ProfileConnectionsMode.following),
            ),
          if (_profile != null) ...[
            const SizedBox(height: 12),
            Row(
              children: [
                Text(context.l10n.timelineTabHome,
                    style: const TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(width: 8),
                if (_outboxLoading)
                  const SizedBox(
                    width: 14,
                    height: 14,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                const Spacer(),
                IconButton(
                  tooltip: 'Aggiorna profilo',
                  onPressed: _loading ? null : _refreshProfileImmediate,
                  icon: const Icon(Icons.person),
                ),
                IconButton(
                  tooltip: 'Aggiorna post',
                  onPressed: _outboxLoading ? null : _refreshOutboxOnly,
                  icon: const Icon(Icons.refresh),
                ),
                if (_isLocalProfile())
                  IconButton(
                    tooltip: _followImportBusy
                        ? 'Import follow in corso'
                        : 'Importa follow da CSV',
                    onPressed:
                        _followImportBusy ? null : () => _pickFollowCsv(api),
                    icon: _followImportBusy
                        ? const SizedBox(
                            width: 18,
                            height: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Icon(Icons.upload_file),
                  ),
                if (_isLocalProfile())
                  IconButton(
                    tooltip: 'Reimporta ultimo CSV',
                    onPressed: _followImportBusy
                        ? null
                        : () => _retryLastFollowImport(api),
                    icon: const Icon(Icons.restart_alt),
                  ),
                if (_isLocalProfile())
                  IconButton(
                    tooltip: 'Esporta follow correnti',
                    onPressed: () => _exportFollowCsv(api),
                    icon: const Icon(Icons.download),
                  ),
              ],
            ),
            if (_isLocalProfile() &&
                _followImportStatusLabel != null &&
                _followImportStatusLabel!.trim().isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 4, bottom: 8),
                child: Text(
                  _followImportStatusLabel!,
                  style: TextStyle(
                    fontSize: 12,
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            if (_outboxError != null && _outbox.isEmpty)
              NetworkErrorCard(
                message: _outboxError,
                onRetry: _refreshOutboxOnly,
                compact: true,
              ),
            if (_outboxLoading && _outbox.isEmpty)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 16),
                child: Center(child: CircularProgressIndicator()),
              ),
            if (!_outboxLoading && _outbox.isEmpty && _outboxLoaded)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 12),
                child: Text(
                  'Nessun post visibile al momento. Se hai pubblicato da poco, attendi la sync o aggiorna.',
                ),
              ),
          ],
          if (_profile == null && !_loading)
            Center(child: Text(context.l10n.listNoItems)),
          if (_loading)
            const Center(
                child: Padding(
                    padding: EdgeInsets.all(16),
                    child: CircularProgressIndicator())),
          if (_featured.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text(context.l10n.profileFeatured,
                style: const TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 8),
            for (final a in _featured)
              TimelineActivityCard(
                  appState: widget.appState, activity: a, elevated: true),
          ],
          if (_outbox.isNotEmpty) ...[
            const SizedBox(height: 8),
            for (final a in _outbox)
              TimelineActivityCard(appState: widget.appState, activity: a),
            const SizedBox(height: 8),
            Center(
              child: _outboxLoadingMore
                  ? const CircularProgressIndicator()
                  : OutlinedButton(
                      onPressed: _outboxNext == null ? null : _loadMoreOutbox,
                      child: Text(_outboxNext == null
                          ? context.l10n.listEnd
                          : context.l10n.listLoadMore),
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
      final page = await ActorRepository.instance
          .fetchOutboxPage(p.outbox, pageUrl: next, limit: 20);
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

  Future<void> _refreshOutboxOnly(
      {ActorProfile? profile, bool silent = false}) async {
    final p = profile ?? _profile;
    if (p == null || p.outbox.trim().isEmpty) return;
    if (_outboxLoading) return;
    if (mounted && !silent) {
      setState(() {
        _outboxLoading = true;
        _outboxError = null;
      });
    } else {
      _outboxLoading = true;
      _outboxError = null;
    }
    try {
      final page =
          await ActorRepository.instance.fetchOutboxPage(p.outbox, limit: 20);
      var usedFallback = false;
      var items = page.items.where(_isProfileActivity).toList();
      if (items.isEmpty) {
        final fallback =
            await _fallbackProfileActivities(p, limit: 20);
        if (fallback.isNotEmpty) {
          items = fallback;
          usedFallback = true;
        }
      }
      if (!mounted) return;
      setState(() {
        _outbox = items;
        _outboxNext = (items.isEmpty || usedFallback) ? null : page.next;
        _outboxLoaded = true;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _outboxError = e.toString();
        _outboxLoaded = true;
      });
    } finally {
      if (mounted) setState(() => _outboxLoading = false);
    }
  }

  Future<List<Map<String, dynamic>>> _fallbackProfileActivities(
    ActorProfile profile, {
    int limit = 20,
  }) async {
    final actorUrl = profile.id.trim();
    if (actorUrl.isEmpty) return const [];
    final merged = <Map<String, dynamic>>[];
    final seen = <String>{};
    void addItems(Iterable<Map<String, dynamic>> source) {
      for (final item in source) {
        if (!_isProfileActivity(item)) continue;
        final key = _activityIdentity(item);
        if (!seen.add(key)) continue;
        merged.add(item);
        if (merged.length >= limit) return;
      }
    }

    addItems(widget.appState.relayTimelineHome);
    if (merged.length < limit) {
      addItems(_activitiesFromRelayEvents(widget.appState.relayEvents));
    }
    if (merged.length < limit && widget.appState.isRunning) {
      try {
        final cfg = widget.appState.config;
        if (cfg != null) {
          final api = CoreApi(config: cfg);
          final federated = await api.fetchTimeline('federated', limit: 120);
          final raw = federated['items'];
          if (raw is List) {
            final fromCore = raw
                .whereType<Map>()
                .map((v) => v.cast<String, dynamic>());
            addItems(fromCore);
          }
        }
      } catch (_) {
        // Best effort fallback.
      }
    }
    merged.sort((a, b) => _activityTimestampMs(b).compareTo(_activityTimestampMs(a)));
    if (merged.length > limit) {
      merged.removeRange(limit, merged.length);
    }
    return merged;
  }

  List<Map<String, dynamic>> _activitiesFromRelayEvents(
      List<Map<String, dynamic>> rows) {
    final out = <Map<String, dynamic>>[];
    for (final row in rows) {
      final activity = row['activity'];
      if (activity is Map) {
        out.add(activity.cast<String, dynamic>());
        continue;
      }
      final payload = row['payload'];
      if (payload is Map) {
        final payloadMap = payload.cast<String, dynamic>();
        final nested = payloadMap['activity'];
        if (nested is Map) {
          out.add(nested.cast<String, dynamic>());
        }
      }
    }
    return out;
  }

  String _activityIdentity(Map<String, dynamic> activity) {
    final id = (activity['id'] as String?)?.trim() ?? '';
    if (id.isNotEmpty) return id;
    final obj = activity['object'];
    if (obj is String && obj.trim().isNotEmpty) return obj.trim();
    if (obj is Map) {
      final map = obj.cast<String, dynamic>();
      final objId = (map['id'] as String?)?.trim() ?? '';
      if (objId.isNotEmpty) return objId;
    }
    return '${activity['type']}-${activity['actor']}-${activity['published']}-${activity['created_at_ms']}';
  }

  int _activityTimestampMs(Map<String, dynamic> activity) {
    final published = (activity['published'] as String?)?.trim() ?? '';
    if (published.isNotEmpty) {
      final dt = DateTime.tryParse(published);
      if (dt != null) return dt.millisecondsSinceEpoch;
    }
    final updated = (activity['updated'] as String?)?.trim() ?? '';
    if (updated.isNotEmpty) {
      final dt = DateTime.tryParse(updated);
      if (dt != null) return dt.millisecondsSinceEpoch;
    }
    final created = activity['created_at_ms'];
    if (created is num) return created.toInt();
    if (created is String) return int.tryParse(created.trim()) ?? 0;
    final cursor = activity['cursor'];
    if (cursor is num) return cursor.toInt();
    if (cursor is String) return int.tryParse(cursor.trim()) ?? 0;
    return 0;
  }

  Future<void> _refreshProfileCounts(ActorProfile profile) async {
    if (_isLocalProfile()) {
      final cfg = widget.appState.config;
      if (cfg != null) {
        try {
          final api = CoreApi(config: cfg);
          final status = await api.fetchMigrationStatus();
          final audit = await api.fetchFollowAudit();
          _followersCount = (status['followers_count'] as num?)?.toInt();
          _followingCount = (status['following_count'] as num?)?.toInt();
          _followingPendingCount =
              (audit['following_pending'] as num?)?.toInt();
          _followingAcceptedCount =
              (audit['following_accepted'] as num?)?.toInt();
          _activeDataDir = (audit['data_dir'] as String?)?.trim();
          _activeDid = (audit['did'] as String?)?.trim();
          _activeUsername = (audit['username'] as String?)?.trim();
          _followAudit = audit;
          return;
        } catch (_) {
          // Fall through to public AP collection counts.
        }
      }
    }

    final followers = profile.followers;
    final following = profile.following;
    if (followers.isNotEmpty) {
      _followersCount =
          await ActorRepository.instance.fetchCollectionCount(followers);
    }
    if (following.isNotEmpty) {
      _followingCount =
          await ActorRepository.instance.fetchCollectionCount(following);
    }
  }

  Future<void> _toggleFollow(CoreApi api) async {
    final p = _profile;
    if (p == null) return;
    if (_followBusy) return;
    final previousStatus = _followingStatus;
    setState(() => _followBusy = true);
    try {
      final following =
          _followingStatus == 'accepted' || _followingStatus == 'pending';
      setState(() => _followingStatus = following ? 'none' : 'pending');
      if (following) {
        await api.unfollow(p.id);
        _stopFollowPoll();
      } else {
        await api.follow(p.id);
        _ensureFollowPoll();
      }
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(context.l10n.settingsOk)));
      await _refreshFollowingStatus();
      // Some servers accept asynchronously; re-check after a short delay.
      await Future<void>.delayed(const Duration(seconds: 2));
      if (mounted) await _refreshFollowingStatus();
    } catch (e) {
      if (!mounted) return;
      setState(() => _followingStatus = previousStatus);
      ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    } finally {
      if (mounted) setState(() => _followBusy = false);
    }
  }

  Future<void> _pickFollowCsv(CoreApi api) async {
    try {
      final file = await openFile(
        acceptedTypeGroups: const [
          XTypeGroup(label: 'CSV', extensions: ['csv']),
        ],
      );
      if (file == null) return;
      setState(() {
        _followImportBusy = true;
        _followImportStatusLabel = 'Import follow in avvio...';
      });
      final bytes = await file.readAsBytes();
      final csv = utf8.decode(bytes, allowMalformed: true);
      final dryRun = await api.importFollowCsv(csv: csv, dryRun: true);
      final candidates = (dryRun['candidates'] as num?)?.toInt() ?? 0;
      final alreadyPresent = (dryRun['already_present'] as num?)?.toInt() ?? 0;
      final invalid = ((dryRun['invalid'] as List?)?.length) ?? 0;
      if (!mounted) return;
      final confirmed = await showDialog<bool>(
            context: context,
            builder: (context) => AlertDialog(
              title: const Text('Ripristina follow da CSV'),
              content: Text(
                'Nuovi target: $candidates\nGia presenti: $alreadyPresent\nInvalidi: $invalid',
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(false),
                  child: const Text('Annulla'),
                ),
                FilledButton(
                  onPressed: () => Navigator.of(context).pop(true),
                  child: const Text('Importa'),
                ),
              ],
            ),
          ) ??
          false;
      if (!confirmed) {
        setState(() {
          _followImportBusy = false;
          _followImportStatusLabel = 'Import follow annullato';
        });
        return;
      }
      final result = await api.importFollowCsv(csv: csv);
      final jobId = result['job_id']?.toString().trim();
      if (!mounted) return;
      setState(() {
        _followImportJobId = (jobId == null || jobId.isEmpty) ? null : jobId;
        _followImportStatusLabel = 'Import queued';
      });
      _startFollowImportPoll(api);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _followImportBusy = false;
        _followImportStatusLabel = 'Import follow fallito';
      });
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Import follow fallito: $e')),
      );
    }
  }

  Future<void> _retryLastFollowImport(CoreApi api) async {
    setState(() {
      _followImportBusy = true;
      _followImportStatusLabel = 'Reimport ultimo CSV in avvio...';
    });
    try {
      final result = await api.retryLastFollowImport();
      final jobId = result['job_id']?.toString().trim();
      if (!mounted) return;
      setState(() {
        _followImportJobId = (jobId == null || jobId.isEmpty) ? null : jobId;
        _followImportStatusLabel = 'Reimport queued';
      });
      _startFollowImportPoll(api);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _followImportBusy = false;
        _followImportStatusLabel = 'Reimport follow fallito';
      });
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Reimport follow fallito: $e')),
      );
    }
  }

  Future<void> _exportFollowCsv(CoreApi api) async {
    try {
      final csv = await api.exportFollowCsv();
      final location = await getSaveLocation(
        suggestedName:
            'following-${DateTime.now().toIso8601String().replaceAll(':', '-')}.csv',
      );
      if (location == null || location.path.trim().isEmpty) return;
      await File(location.path).writeAsString(csv);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('CSV follow salvato in ${location.path}')),
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Export follow fallito: $e')),
      );
    }
  }

  void _startFollowImportPoll(CoreApi api) {
    _followImportPoll?.cancel();
    _followImportPoll = Timer.periodic(const Duration(seconds: 2), (_) async {
      final jobId = _followImportJobId;
      if (jobId == null || jobId.isEmpty) {
        _followImportPoll?.cancel();
        return;
      }
      try {
        final status = await api.fetchFollowImportStatus(jobId: jobId);
        if (!mounted) return;
        final stateLabel = status['status']?.toString() ?? 'unknown';
        final imported = (status['imported'] as num?)?.toInt() ?? 0;
        final failed = (status['failed'] as num?)?.toInt() ?? 0;
        final invalid = (status['invalid'] as num?)?.toInt() ?? 0;
        setState(() {
          _followImportStatusLabel =
              'Import follow: $stateLabel, imported=$imported, failed=$failed, invalid=$invalid';
        });
        if (stateLabel == 'completed' || stateLabel == 'failed') {
          _followImportPoll?.cancel();
          setState(() => _followImportBusy = false);
          await _refreshProfileImmediate();
          await _refreshOutboxOnly(silent: true);
        }
      } catch (e) {
        if (!mounted) return;
        _followImportPoll?.cancel();
        setState(() {
          _followImportBusy = false;
          _followImportStatusLabel = 'Import follow: errore stato job';
        });
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Errore stato import follow: $e')),
        );
      }
    });
  }

  void _ensureFollowPoll() {
    if (_followPoll != null) return;
    _followPoll = Timer.periodic(
        const Duration(seconds: 6), (_) => _refreshFollowingStatus());
  }

  void _stopFollowPoll() {
    _followPoll?.cancel();
    _followPoll = null;
  }

  void _openConnections(ProfileConnectionsMode mode) {
    final profile = _profile;
    if (profile == null) return;
    final url = mode == ProfileConnectionsMode.followers
        ? profile.followers
        : profile.following;
    if (url.trim().isEmpty) return;
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => ProfileConnectionsScreen(
          appState: widget.appState,
          collectionUrl: url,
          mode: mode,
        ),
      ),
    );
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
    required this.followingPendingCount,
    required this.followingAcceptedCount,
    required this.activeDataDir,
    required this.activeDid,
    required this.activeUsername,
    required this.followAudit,
    required this.onOpenFollowers,
    required this.onOpenFollowing,
  });

  final ActorProfile profile;
  final String followingStatus;
  final bool followBusy;
  final VoidCallback onToggleFollow;
  final int? followersCount;
  final int? followingCount;
  final int? followingPendingCount;
  final int? followingAcceptedCount;
  final String? activeDataDir;
  final String? activeDid;
  final String? activeUsername;
  final Map<String, dynamic>? followAudit;
  final VoidCallback onOpenFollowers;
  final VoidCallback onOpenFollowing;

  @override
  Widget build(BuildContext context) {
    final isPending = followingStatus == 'pending';
    final isFollowing = followingStatus == 'accepted';
    final label = isPending
        ? context.l10n.profileFollowPending
        : (isFollowing
            ? context.l10n.settingsUnfollow
            : context.l10n.settingsFollow);
    final lastImport = followAudit?['latest_follow_import_job'];
    final importedBaseline = (lastImport is Map
            ? (lastImport['imported'] as num?)?.toInt()
            : null) ??
        0;
    final effectiveFollowing =
        (followingCount ?? 0) + (followingPendingCount ?? 0);
    final showDropWarning = followingCount != null &&
        importedBaseline > 0 &&
        effectiveFollowing < importedBaseline;
    final fields = profile.fields;
    final hasBanner = profile.imageUrl.trim().isNotEmpty;
    final acct = profile.preferredUsername.isNotEmpty
        ? '@${profile.preferredUsername}'
        : profile.id;
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
                  color: Theme.of(context)
                      .colorScheme
                      .surfaceContainerHighest
                      .withAlpha(120),
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(
                      color: Theme.of(context)
                          .colorScheme
                          .outlineVariant
                          .withAlpha(120)),
                ),
                child: InkWell(
                  onTap: () => openUrlExternal(profile.movedTo),
                  child: Row(
                    children: [
                      const Icon(Icons.arrow_forward, size: 18),
                      const SizedBox(width: 8),
                      Expanded(
                          child: Text(
                              context.l10n.profileMovedTo(profile.movedTo))),
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
                      Container(
                          color: Theme.of(context)
                              .colorScheme
                              .surfaceContainerHighest),
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
                            style: const TextStyle(
                                fontWeight: FontWeight.w800, fontSize: 20),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            acct,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                                color: Theme.of(context)
                                    .colorScheme
                                    .onSurface
                                    .withAlpha(200)),
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
                        Text(profile.displayName,
                            style: const TextStyle(
                                fontWeight: FontWeight.w800, fontSize: 16)),
                        const SizedBox(height: 2),
                        Text(
                          acct,
                          style: TextStyle(
                              color: Theme.of(context)
                                  .colorScheme
                                  .onSurface
                                  .withAlpha(179)),
                        ),
                      ],
                      if (profile.url.trim().isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 4),
                          child: Text(
                            profile.url,
                            style: TextStyle(
                                color: Theme.of(context).colorScheme.primary,
                                fontSize: 12),
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
                          ? const SizedBox(
                              width: 16,
                              height: 16,
                              child: CircularProgressIndicator(strokeWidth: 2))
                          : Text(label),
                    ),
                    if (isPending) ...[
                      const SizedBox(height: 6),
                      Container(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 8, vertical: 4),
                        decoration: BoxDecoration(
                          color: Theme.of(context)
                              .colorScheme
                              .surfaceContainerHighest
                              .withAlpha(120),
                          borderRadius: BorderRadius.circular(999),
                          border: Border.all(
                            color: Theme.of(context)
                                .colorScheme
                                .outlineVariant
                                .withAlpha(120),
                          ),
                        ),
                        child: Text(
                          context.l10n.profileFollowPending,
                          style: const TextStyle(
                              fontSize: 11, fontWeight: FontWeight.w700),
                        ),
                      ),
                    ],
                    const SizedBox(height: 6),
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (followersCount != null)
                          _StatChip(
                              label: context.l10n.profileFollowers,
                              value: followersCount!,
                              onTap: onOpenFollowers),
                        if (followersCount != null && followingCount != null)
                          const SizedBox(width: 6),
                        if (followingCount != null)
                          _StatChip(
                              label: context.l10n.profileFollowing,
                              value: followingCount!,
                              onTap: onOpenFollowing),
                      ],
                    ),
                    if (followingPendingCount != null &&
                        followingPendingCount! > 0) ...[
                      const SizedBox(height: 6),
                      Text(
                        'In attesa: $followingPendingCount · Accettati: ${followingAcceptedCount ?? 0}',
                        style: TextStyle(
                            color: Theme.of(context)
                                .colorScheme
                                .onSurface
                                .withAlpha(180),
                            fontSize: 12),
                      ),
                    ],
                  ],
                ),
              ],
            ),
            if (showDropWarning) ...[
              const SizedBox(height: 10),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: Theme.of(context)
                      .colorScheme
                      .errorContainer
                      .withAlpha(160),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Text(
                  'Anomalia locale: seguiti attivi $effectiveFollowing, ultimo import noto $importedBaseline.',
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onErrorContainer,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
            if (profile.summary.isNotEmpty) ...[
              const SizedBox(height: 10),
              HtmlWidget(profile.summary),
            ],
            if ((activeUsername?.isNotEmpty ?? false) ||
                (activeDid?.isNotEmpty ?? false) ||
                (activeDataDir?.isNotEmpty ?? false)) ...[
              const SizedBox(height: 12),
              const Text('Storage attivo',
                  style: TextStyle(fontWeight: FontWeight.w700)),
              const SizedBox(height: 6),
              if (activeUsername?.isNotEmpty ?? false)
                SelectableText('Username: $activeUsername'),
              if (activeDid?.isNotEmpty ?? false)
                SelectableText('DID: $activeDid'),
              if (activeDataDir?.isNotEmpty ?? false)
                SelectableText('Data dir: $activeDataDir'),
            ],
            if (profile.aliases.isNotEmpty) ...[
              const SizedBox(height: 10),
              Text(context.l10n.profileAliases,
                  style: const TextStyle(fontWeight: FontWeight.w600)),
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
    final matches =
        RegExp('href\\s*=\\s*([\'"])([^\'"]+)\\1', caseSensitive: false)
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
  const _StatChip({required this.label, required this.value, this.onTap});

  final String label;
  final int value;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      borderRadius: BorderRadius.circular(999),
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        decoration: BoxDecoration(
          color: Theme.of(context)
              .colorScheme
              .surfaceContainerHighest
              .withAlpha(120),
          borderRadius: BorderRadius.circular(999),
          border: Border.all(
              color:
                  Theme.of(context).colorScheme.outlineVariant.withAlpha(120)),
        ),
        child: Text(
          '$label $value',
          style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w700),
        ),
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
