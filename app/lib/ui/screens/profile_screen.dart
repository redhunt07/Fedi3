import '../../services/app_activity.dart';
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:flutter_widget_from_html_core/flutter_widget_from_html_core.dart';

import '../../core/core_api.dart';
import '../../l10n/l10n_ext.dart';
import '../../services/actor_repository.dart';
import '../../state/app_state.dart';
import '../widgets/network_error_card.dart';
import '../widgets/status_avatar.dart';
import '../widgets/fedi3_ui.dart';
import '../widgets/timeline_activity_card.dart';
import '../theme/ui_tokens.dart';
import '../utils/open_url.dart';
import 'profile_edit_screen.dart';
import 'relations_screen.dart';

class ProfileScreen extends StatefulWidget {
  const ProfileScreen(
      {super.key,
      required this.appState,
      required this.actorUrl,
      this.onOpenSettings});

  final AppState appState;
  final String actorUrl;

  /// Present only for the signed-in account: opens the settings page.
  final VoidCallback? onOpenSettings;

  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  ActorProfile? _profile;
  String? _error;
  bool _loading = false;
  List<Map<String, dynamic>> _outbox = const [];
  String? _outboxNext;
  bool _loadingMore = false;
  String _followingStatus = 'none';
  bool _followBusy = false;
  int? _followersCount;
  int? _followingCount;
  int _profileTab = 0;
  Timer? _followPoll;
  Timer? _relationsPoll;
  StreamSubscription<bool>? _activity;
  bool _refreshingRelations = false;

  @override
  void initState() {
    super.initState();
    _load();
    _relationsPoll = AppActivity.periodic(
        const Duration(seconds: 15), (_) => _refreshRelations());
    _activity = AppActivity.changes.listen((active) {
      if (active && mounted) _refreshRelations();
    });
  }

  @override
  void dispose() {
    _followPoll?.cancel();
    _relationsPoll?.cancel();
    _activity?.cancel();
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
      unawaited(_refreshRelations());
      if (p?.outbox.isNotEmpty ?? false) {
        final page = await ActorRepository.instance
            .fetchOutboxPage(p!.outbox, limit: 20);
        if (!mounted) return;
        setState(() {
          _outbox = page.items;
          _outboxNext = page.next;
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _refreshRelations() async {
    final p = _profile;
    if (p == null || _refreshingRelations || !widget.appState.isRunning) return;
    _refreshingRelations = true;
    Future<void> count(String url, bool followers) async {
      if (url.isEmpty) return;
      try {
        final value = await ActorRepository.instance.fetchCollectionCount(url, config: widget.appState.config);
        if (!mounted) return;
        setState(() {
          if (followers) {
            _followersCount = value;
          } else {
            _followingCount = value;
          }
        });
      } catch (_) {/* A remote count must not block the profile. */}
    }

    try {
      await Future.wait([
        _refreshFollowingStatus(),
        count(p.followers, true),
        count(p.following, false)
      ]);
    } finally {
      _refreshingRelations = false;
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
          if (widget.onOpenSettings != null)
            IconButton(
                tooltip: context.l10n.navSettings,
                onPressed: widget.onOpenSettings,
                icon: const Icon(Icons.settings_outlined)),
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
              isSelf: _isSelf(_profile!),
              onOpenFollowers: _profile!.followers.isEmpty
                  ? null
                  : () => _openRelations(
                      _profile!.followers, context.l10n.profileFollowers),
              onOpenFollowing: _profile!.following.isEmpty
                  ? null
                  : () => _openRelations(
                      _profile!.following, context.l10n.profileFollowing),
              onEdit: _isSelf(_profile!) ? _openEditProfile : null,
            ),
          if (_profile != null) ...[
            const SizedBox(height: 14),
            Align(
              alignment: Alignment.centerLeft,
              child: SegmentedButton<int>(
                showSelectedIcon: false,
                style: ButtonStyle(
                  visualDensity: VisualDensity.compact,
                  backgroundColor: WidgetStateProperty.resolveWith((states) {
                    if (states.contains(WidgetState.selected)) {
                      return Theme.of(context)
                          .colorScheme
                          .primary
                          .withAlpha(70);
                    }
                    return Colors.transparent;
                  }),
                  foregroundColor: WidgetStatePropertyAll(
                      Theme.of(context).colorScheme.onSurface),
                  side: WidgetStatePropertyAll(BorderSide(
                      color: Theme.of(context).colorScheme.outlineVariant)),
                ),
                segments: [
                  ButtonSegment(
                      value: 0, label: Text(context.l10n.searchTabPosts)),
                  ButtonSegment(
                      value: 1, label: Text(context.l10n.timelineFilterReply)),
                  ButtonSegment(
                      value: 2, label: Text(context.l10n.timelineFilterMedia)),
                ],
                selected: {_profileTab},
                onSelectionChanged: (s) =>
                    setState(() => _profileTab = s.first),
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
          if (_profile != null) ...[
            const SizedBox(height: 12),
            if (_filteredOutbox().isEmpty)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 20),
                child: EmptyState(
                  icon: Icons.article_outlined,
                  title: context.l10n.listNoItems,
                ),
              )
            else if (_profileTab == 2)
              _mediaGrid(context)
            else ...[
              for (final a in _filteredOutbox())
                TimelineActivityCard(appState: widget.appState, activity: a),
              if (_outboxNext != null)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 12),
                  child: Center(
                    child: OutlinedButton.icon(
                      onPressed: _loadingMore ? null : _loadMore,
                      icon: _loadingMore
                          ? const SizedBox(
                              width: 16,
                              height: 16,
                              child: CircularProgressIndicator(strokeWidth: 2))
                          : const Icon(Icons.expand_more_rounded),
                      label: Text(context.l10n.profileLoadMore),
                    ),
                  ),
                ),
            ],
          ],
        ],
      ),
    );
  }

  Map<String, dynamic>? _noteMap(Map<String, dynamic> activity) {
    final obj = activity['object'];
    if (obj is String) return null;
    if (obj is! Map) return null;
    final m = obj.cast<String, dynamic>();
    final inner = m['object'];
    if (inner is Map) return inner.cast<String, dynamic>();
    return m;
  }

  List<Map<String, dynamic>> _filteredOutbox() {
    final notes = _outbox.where(_isNoteActivity).toList(growable: false);
    return notes.where((a) {
      final note = _noteMap(a);
      if (note == null) return false;
      final irt = (note['inReplyTo'] as String?)?.trim() ?? '';
      final attachment = note['attachment'];
      final hasMedia = attachment is List && attachment.isNotEmpty;
      return switch (_profileTab) {
        0 => irt.isEmpty,
        1 => irt.isNotEmpty,
        _ => hasMedia,
      };
    }).toList(growable: false);
  }

  Future<void> _loadMore() async {
    final p = _profile;
    final next = _outboxNext;
    if (p == null || next == null) return;
    setState(() => _loadingMore = true);
    try {
      final page = await ActorRepository.instance
          .fetchOutboxPage(p.outbox, limit: 20, pageUrl: next);
      if (!mounted) return;
      final seen = _outbox.map((a) => a['id']?.toString()).toSet();
      setState(() {
        _outbox = [
          ..._outbox,
          ...page.items.where((a) => seen.add(a['id']?.toString())),
        ];
        _outboxNext = page.next;
      });
    } catch (_) {
      // keep current page on failure
    } finally {
      if (mounted) setState(() => _loadingMore = false);
    }
  }

  List<Map<String, dynamic>> _mediaItems() {
    final out = <Map<String, dynamic>>[];
    for (final a in _filteredOutbox()) {
      final note = _noteMap(a);
      final att = note?['attachment'];
      if (att is! List) continue;
      for (final item in att) {
        if (item is Map) out.add(item.cast<String, dynamic>());
      }
    }
    return out;
  }

  Widget _mediaGrid(BuildContext context) {
    final items = _mediaItems();
    if (items.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 20),
        child: EmptyState(
          icon: Icons.photo_library_outlined,
          title: context.l10n.listNoItems,
        ),
      );
    }
    return GridView.builder(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
        crossAxisCount: 3,
        crossAxisSpacing: 8,
        mainAxisSpacing: 8,
        childAspectRatio: 1,
      ),
      itemCount: items.length,
      itemBuilder: (context, index) {
        final item = items[index];
        final url = (item['url'] as String?)?.trim() ?? '';
        final type = (item['mediaType'] as String?)?.trim() ?? '';
        final isVideo = type.startsWith('video/');
        final isAudio = type.startsWith('audio/');
        final placeholder = Container(
          decoration: BoxDecoration(
            color: Theme.of(context).colorScheme.surfaceContainerHighest,
          ),
          child: Icon(
            isVideo
                ? Icons.play_circle_outline_rounded
                : isAudio
                    ? Icons.graphic_eq_rounded
                    : Icons.image_outlined,
            color: Theme.of(context).colorScheme.onSurfaceVariant,
          ),
        );
        return InkWell(
          borderRadius: BorderRadius.circular(12),
          onTap: url.isEmpty
              ? null
              : () => launchUrl(Uri.parse(url),
                  mode: LaunchMode.externalApplication),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(12),
            child: Stack(
              fit: StackFit.expand,
              children: [
                if (url.isNotEmpty && !isVideo && !isAudio)
                  Image.network(
                    url,
                    fit: BoxFit.cover,
                    errorBuilder: (_, __, ___) => placeholder,
                    loadingBuilder: (context, child, progress) =>
                        progress == null ? child : placeholder,
                  )
                else
                  placeholder,
                if (isVideo)
                  const Align(
                    alignment: Alignment.center,
                    child: Icon(Icons.play_arrow_rounded,
                        size: 36, color: Colors.white),
                  ),
              ],
            ),
          ),
        );
      },
    );
  }

  void _openEditProfile() {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => ProfileEditScreen(appState: widget.appState),
      ),
    );
  }

  bool _isNoteActivity(Map<String, dynamic> activity) {
    final type = (activity['type'] as String?)?.trim() ?? '';
    if (type != 'Create' && type != 'Announce') return false;
    final obj = activity['object'];
    if (obj is! Map) return obj is String;
    final m = obj.cast<String, dynamic>();
    if (type == 'Create') {
      final inner = m['object'];
      if (inner is Map) {
        return (inner['type'] as String?) == 'Note';
      }
      return (m['type'] as String?) == 'Note';
    }
    if ((m['type'] as String?) == 'Note') return true;
    final inner = m['object'];
    if (inner is Map) return (inner['type'] as String?) == 'Note';
    return inner is String && inner.trim().isNotEmpty;
  }

  bool _isSelf(ActorProfile profile) {
    final cfg = widget.appState.config;
    if (cfg == null) return false;
    final own =
        '${cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '')}/users/${cfg.username.trim()}';
    return own == profile.id.trim().replaceAll(RegExp(r'/+$'), '');
  }

  void _openRelations(String collectionUrl, String title) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => RelationsScreen(
          appState: widget.appState,
          actorUrl: _profile!.id,
          collectionUrl: collectionUrl,
          title: title,
        ),
      ),
    );
  }

  Future<void> _toggleFollow(CoreApi api) async {
    final p = _profile;
    if (p == null) return;
    if (_followBusy) return;
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
      ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.settingsErr(e.toString()))));
    } finally {
      if (mounted) setState(() => _followBusy = false);
    }
  }

  void _ensureFollowPoll() {
    if (_followPoll != null) return;
    _followPoll = AppActivity.periodic(
        const Duration(seconds: 6), (_) => _refreshFollowingStatus());
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
    required this.isSelf,
    required this.onOpenFollowers,
    required this.onOpenFollowing,
    required this.onEdit,
  });

  final ActorProfile profile;
  final String followingStatus;
  final bool followBusy;
  final VoidCallback onToggleFollow;
  final int? followersCount;
  final int? followingCount;
  final bool isSelf;
  final VoidCallback? onOpenFollowers;
  final VoidCallback? onOpenFollowing;
  final VoidCallback? onEdit;

  @override
  Widget build(BuildContext context) {
    final following =
        followingStatus == 'accepted' || followingStatus == 'pending';
    final label =
        following ? context.l10n.settingsUnfollow : context.l10n.settingsFollow;
    final fields = profile.fields;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(UiTokens.padCard),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Stack(
              clipBehavior: Clip.none,
              children: [
                ClipRRect(
                  borderRadius: BorderRadius.circular(UiTokens.radiusCardSm),
                  child: SizedBox(
                    height: 118,
                    width: double.infinity,
                    child: CustomPaint(
                      painter: _ProfileBannerPainter(
                        primary: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                  ),
                ),
                Positioned(
                  left: 16,
                  bottom: -32,
                  child: Container(
                    padding: const EdgeInsets.all(3),
                    decoration: const BoxDecoration(
                      shape: BoxShape.circle,
                      gradient: LinearGradient(
                        begin: Alignment.topLeft,
                        end: Alignment.bottomRight,
                        colors: [
                          Color(UiTokens.brandViolet),
                          Color(UiTokens.brandCyan),
                        ],
                      ),
                      boxShadow: [
                        BoxShadow(
                          color: Color(0x667C6CF6),
                          blurRadius: 22,
                          offset: Offset(0, 8),
                        ),
                      ],
                    ),
                    child: Container(
                      padding: const EdgeInsets.all(2),
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: Theme.of(context).colorScheme.surface,
                      ),
                      child: _Avatar(
                        url: profile.iconUrl,
                        size: 78,
                        showStatus: true,
                        statusKey: profile.statusKey,
                      ),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 40),
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
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(profile.displayName,
                    style: Theme.of(context)
                        .textTheme
                        .titleLarge
                        ?.copyWith(fontWeight: FontWeight.w800)),
                const SizedBox(height: 2),
                Text(
                  _fullHandle(profile),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      color: Theme.of(context)
                          .colorScheme
                          .onSurface
                          .withAlpha(179)),
                ),
                if (_secondaryLink(profile) != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text(
                      _secondaryLink(profile)!,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          color: Theme.of(context).colorScheme.primary,
                          fontSize: 12),
                    ),
                  ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    if (isSelf)
                      OutlinedButton(
                        onPressed: onEdit,
                        child: Text(context.l10n.profileEditAction),
                      )
                    else
                      FilledButton(
                        onPressed: followBusy ? null : onToggleFollow,
                        child: followBusy
                            ? const SizedBox(
                                width: 16,
                                height: 16,
                                child:
                                    CircularProgressIndicator(strokeWidth: 2))
                            : Text(label),
                      ),
                    if (followersCount != null)
                      _StatChip(
                          label: context.l10n.profileFollowers,
                          value: followersCount!,
                          onTap: onOpenFollowers),
                    if (followingCount != null)
                      _StatChip(
                          label: context.l10n.profileFollowing,
                          value: followingCount!,
                          onTap: onOpenFollowing),
                  ],
                ),
              ],
            ),
            if (profile.summary.isNotEmpty) ...[
              const SizedBox(height: 10),
              HtmlWidget(profile.summary, onTapUrl: UrlPolicy.handleTap),
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
                      Expanded(
                          child: HtmlWidget(f.value,
                              onTapUrl: UrlPolicy.handleTap)),
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

class _ProfileBannerPainter extends CustomPainter {
  const _ProfileBannerPainter({required this.primary});

  final Color primary;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final base = Paint()
      ..shader = const LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [
          Color(UiTokens.brandViolet),
          Color(0xFF5B8DF6),
          Color(UiTokens.brandCyan),
        ],
        stops: [0, 0.5, 1],
      ).createShader(rect);
    canvas.drawRect(rect, base);

    final glow = Paint()
      ..color = Colors.white.withAlpha(38)
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 34);
    canvas.drawCircle(Offset(size.width * 0.22, size.height * 0.25),
        size.height * 0.55, glow);
    canvas.drawCircle(Offset(size.width * 0.82, size.height * 0.78),
        size.height * 0.62, glow);

    final accent = Paint()
      ..color = primary.withAlpha(80)
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 24);
    canvas.drawCircle(Offset(size.width * 0.62, size.height * 0.18),
        size.height * 0.4, accent);

    final dot = Paint()..color = Colors.white.withAlpha(90);
    for (var i = 0; i < 18; i++) {
      final x = size.width * (0.05 + 0.9 * ((i * 0.37) % 1.0));
      final y = size.height * (0.12 + 0.76 * ((i * 0.61) % 1.0));
      canvas.drawCircle(Offset(x, y), 1.2, dot);
    }
  }

  @override
  bool shouldRepaint(covariant _ProfileBannerPainter oldDelegate) =>
      oldDelegate.primary != primary;
}

class _StatChip extends StatelessWidget {
  const _StatChip(
      {required this.label, required this.value, required this.onTap});

  final String label;
  final int value;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final content = Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: Theme.of(context)
            .colorScheme
            .surfaceContainerHighest
            .withAlpha(120),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(
            color: Theme.of(context).colorScheme.outlineVariant.withAlpha(120)),
      ),
      child: Text(
        '$label $value',
        style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w700),
      ),
    );
    if (onTap == null) return content;
    return InkWell(
      borderRadius: BorderRadius.circular(999),
      onTap: onTap,
      child: content,
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

/// Full Fediverse handle (`@user@host`) for a profile.
String _fullHandle(ActorProfile profile) {
  final username = profile.preferredUsername;
  final id = profile.id;
  final host = Uri.tryParse(id)?.host ?? '';
  if (username.isEmpty) return id;
  return host.isEmpty ? '@$username' : '@$username@$host';
}

/// The profile URL, shown only when it is not already implied by the handle.
String? _secondaryLink(ActorProfile profile) {
  final url = profile.url.trim();
  final id = profile.id;
  if (url.isEmpty || url == id) return null;
  return url;
}
