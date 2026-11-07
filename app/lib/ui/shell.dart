import '../services/app_activity.dart';
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../core/core_api.dart';
import '../state/app_state.dart';
import '../state/peer_presence_store.dart';
import '../l10n/l10n_ext.dart';
import '../model/core_config.dart';
import '../model/chat_models.dart';
import '../services/core_event_stream.dart';
import '../services/announcements_service.dart';
import '../services/notification_service.dart';
import '../services/sound_service.dart';
import 'screens/announcements_screen.dart';
import 'screens/compose_screen.dart';
import 'screens/notifications_screen.dart';
import 'screens/chat_thread_screen.dart';
import 'screens/profile_screen.dart';
import 'screens/relays_screen.dart';
import 'screens/search_screen.dart';
import 'screens/settings_screen.dart';
import 'screens/timelines_screen.dart';
import 'screens/chat_screen.dart';
import 'screens/call_screen.dart';
import 'widgets/fedi3_ui.dart';
import 'widgets/right_sidebar.dart';
import 'theme/ui_tokens.dart';

class Shell extends StatefulWidget {
  const Shell({super.key, required this.appState});

  final AppState appState;

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> with SingleTickerProviderStateMixin {
  int _index = 0;
  final GlobalKey _searchScreenKey = GlobalKey();
  late final List<Widget> _pages;
  StreamSubscription<CoreEvent>? _chatStream;
  Timer? _chatDebounce;
  Timer? _chatRetry;
  CoreConfig? _chatStreamConfig;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;
  Timer? _announcementsTimer;
  Timer? _announcementsInitialCheck;
  bool _announcementsUnread = false;
  bool _announcementsChecking = false;
  StreamSubscription<String>? _notificationTaps;
  Timer? _callPollTimer;
  int _lastCallSignalMs = 0;
  int _lastCallSignalId = 0;
  int _idleCallPolls = 0;
  int _callPollTicks = 0;
  bool _pollingCalls = false;
  String? _incomingCallId;
  bool _callPromptOpen = false;
  late final AnimationController _sectionFade = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 220),
    value: 1,
  );

  @override
  void initState() {
    super.initState();
    _notificationTaps = NotificationService.taps.listen(_openNotifiedThread);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final pending = NotificationService.pendingThread;
      if (pending != null) _openNotifiedThread(pending);
    });
    _pages = <Widget>[
      TimelinesScreen(appState: widget.appState),
      SearchScreen(key: _searchScreenKey, appState: widget.appState),
      ChatScreen(appState: widget.appState),
      NotificationsScreen(appState: widget.appState),
      RelaysScreen(appState: widget.appState),
      SettingsScreen(appState: widget.appState),
      if (widget.appState.config != null)
        ProfileScreen(
          appState: widget.appState,
          actorUrl: _selfActorUrl(widget.appState.config!),
        ),
    ];
    _lastRunning = widget.appState.isRunning;
    _appStateListener = () {
      final running = widget.appState.isRunning;
      final cfg = widget.appState.config;
      final configChanged = !identical(_chatStreamConfig, cfg);
      if (!running) {
        _stopChatStream();
        _stopCallPolling();
      } else if (running && (!_lastRunning || configChanged)) {
        _startChatStream();
        _startCallPolling();
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);
    if (widget.appState.isRunning) {
      _startChatStream();
      _startCallPolling();
    }
    _announcementsTimer = AppActivity.periodic(
        const Duration(hours: 6), (_) => _checkAnnouncements());
    _announcementsInitialCheck =
        Timer(const Duration(seconds: 4), _checkAnnouncements);
  }

  Future<void> _checkAnnouncements() async {
    if (_announcementsChecking) return;
    _announcementsChecking = true;
    try {
      final update = await AnnouncementsService.checkForUpdates(
        languageCode:
            WidgetsBinding.instance.platformDispatcher.locale.languageCode,
      );
      if (!mounted) return;
      if (update.hasNew) {
        setState(() => _announcementsUnread = true);
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(context.l10n.announcementsNewAlert),
            action: SnackBarAction(
              label: context.l10n.announcementsOpen,
              onPressed: _openAnnouncements,
            ),
          ),
        );
      }
    } catch (_) {
      // Offline is fine: the cached copy stays available in the screen.
    } finally {
      _announcementsChecking = false;
    }
  }

  Future<void> _openAnnouncements() async {
    setState(() => _announcementsUnread = false);
    await Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => const AnnouncementsScreen()),
    );
  }

  @override
  void dispose() {
    _notificationTaps?.cancel();
    unawaited(SoundService.stopCall());
    _chatStream?.cancel();
    _chatDebounce?.cancel();
    _chatRetry?.cancel();
    _announcementsTimer?.cancel();
    _announcementsInitialCheck?.cancel();
    _callPollTimer?.cancel();
    widget.appState.removeListener(_appStateListener);
    _sectionFade.dispose();
    super.dispose();
  }

  Future<void> _openNotifiedThread(String threadId) async {
    if (!mounted || !widget.appState.isRunning) return;
    NotificationService.pendingThread = null;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    try {
      // Resolve a local thread rather than trusting a URL from the payload.
      await CoreApi(config: cfg)
          .fetchChatMessages(threadId: threadId, limit: 1);
      if (!mounted) return;
      await Navigator.of(context).push(MaterialPageRoute<void>(
          builder: (_) => ChatThreadScreen(
              appState: widget.appState,
              threadId: threadId,
              title: context.l10n.chatTitle)));
    } catch (_) {}
  }

  void _startChatStream() {
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_chatStreamConfig, cfg) && _chatStream != null) return;
    _chatStreamConfig = cfg;
    _chatStream?.cancel();
    _chatStream =
        CoreEventStream(config: cfg).stream(kind: 'chat').listen((ev) {
      if (!mounted) return;
      if (ev.kind != 'chat') return;
      if (ev.activityType == 'call') {
        _pollIncomingCall();
        return;
      }
      if (ev.activityType != 'message' && ev.activityType != 'system') return;
      final lastSeen = widget.appState.prefs.lastChatSeenMs;
      if (ev.tsMs <= lastSeen) return;
      _chatDebounce?.cancel();
      _chatDebounce = Timer(const Duration(milliseconds: 350), () {
        if (!mounted) return;
        final onChatTab = _index == 2;
        if (!onChatTab) {
          widget.appState.incrementUnreadChats();
          _showChatSnack();
          unawaited(SoundService.playChat());
          NotificationService.showChatNotification(
            title: context.l10n.chatTitle,
            body: context.l10n.chatNewMessageBody,
            threadId: ev.activityId,
          );
        }
      });
    }, onError: (_) => _scheduleChatRetry(), onDone: _scheduleChatRetry);
  }

  void _stopChatStream() {
    _chatRetry?.cancel();
    _chatStream?.cancel();
    _chatStream = null;
  }

  void _scheduleChatRetry() {
    if (!mounted) return;
    _chatStream = null;
    if (!widget.appState.isRunning) return;
    _chatRetry?.cancel();
    _chatRetry = Timer(const Duration(seconds: 2), () {
      if (!mounted) return;
      _startChatStream();
    });
  }

  void _startCallPolling() {
    if (!widget.appState.isRunning || _callPollTimer != null) return;
    if (_lastCallSignalMs == 0) {
      _lastCallSignalMs = DateTime.now().millisecondsSinceEpoch -
          const Duration(seconds: 30).inMilliseconds;
    }
    _callPollTimer = AppActivity.periodic(const Duration(seconds: 3), (_) {
      if (++_callPollTicks % (_idleCallPolls >= 3 ? 5 : 1) == 0) {
        _pollIncomingCall();
      }
    });
    // ModalRoute is an inherited dependency and cannot be read in initState.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _callPollTimer != null) _pollIncomingCall();
    });
  }

  void _stopCallPolling() {
    _callPollTimer?.cancel();
    _callPollTimer = null;
    _lastCallSignalMs = 0;
    _lastCallSignalId = 0;
    _idleCallPolls = 0;
  }

  Future<void> _pollIncomingCall() async {
    if (!mounted ||
        !widget.appState.isRunning ||
        _callPromptOpen ||
        _pollingCalls) {
      return;
    }
    // When a pushed route (e.g. the chat thread) is active, that screen owns
    // incoming-call polling; avoid showing two prompts for the same call.
    final route = ModalRoute.of(context);
    if (route != null && !route.isCurrent) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    final api = CoreApi(config: cfg);
    final self = _selfActorUrl(cfg);
    _pollingCalls = true;
    try {
      final signals = await api.fetchGlobalChatCallSignals(
          afterId: _lastCallSignalId, afterMs: _lastCallSignalMs, limit: 200);
      _idleCallPolls = signals.isEmpty ? _idleCallPolls + 1 : 0;
      final ended = signals
          .where((s) => s.action == 'leave' || s.action == 'reject')
          .map((s) => s.callId)
          .toSet();
      // Advance in ID order: two events in the same millisecond are distinct.
      for (final signal in signals) {
        if (signal.id > _lastCallSignalId) _lastCallSignalId = signal.id;
        if (signal.action != 'invite' ||
            signal.fromActor == self ||
            ended.contains(signal.callId) ||
            signal.callId == _incomingCallId ||
            DateTime.now().millisecondsSinceEpoch - signal.createdAtMs >
                60000) {
          continue;
        }
        _incomingCallId = signal.callId;
        await _showGlobalIncomingCall(
            threadId: signal.threadId,
            title: signal.threadTitle.isNotEmpty
                ? signal.threadTitle
                : signal.fromActor.split('/').last,
            media: signal.data['media']?.toString() ?? 'audio',
            signal: signal);
        return;
      }
    } catch (_) {
      // Polling is best-effort; the chat thread screen still polls when open.
    } finally {
      _pollingCalls = false;
    }
  }

  Future<void> _showGlobalIncomingCall({
    required String threadId,
    required String title,
    required String media,
    required ChatCallSignal signal,
  }) async {
    if (_callPromptOpen || !mounted) return;
    _callPromptOpen = true;
    // Loop the dedicated ringtone while the incoming-call prompt is open.
    unawaited(SoundService.playCallLoop());
    final from = signal.fromActor;
    final accepted = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.callIncoming),
        content: Row(
          children: [
            CircleAvatar(
              radius: 24,
              backgroundColor:
                  Theme.of(context).colorScheme.primary.withAlpha(35),
              child: Icon(
                media == 'video' ? Icons.videocam_rounded : Icons.call_rounded,
                color: Theme.of(context).colorScheme.primary,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(title),
                  Text(
                    media == 'video'
                        ? context.l10n.callIncomingVideo
                        : context.l10n.callIncomingAudio,
                    style: Theme.of(context).textTheme.labelSmall,
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(context.l10n.callDecline),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l10n.callAccept),
          ),
        ],
      ),
    );
    await SoundService.stopCall();
    _callPromptOpen = false;
    if (accepted != true) {
      _incomingCallId = null;
      try {
        await CoreApi(config: widget.appState.config!).sendChatCallSignal(
          threadId: threadId,
          callId: signal.callId,
          action: 'decline',
          to: from,
        );
      } catch (_) {}
      return;
    }
    if (!mounted) return;
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => CallScreen(
          appState: widget.appState,
          threadId: threadId,
          title: title,
          mediaType: media,
          isGroup: false,
          incoming: true,
          callId: signal.callId,
          remoteActor: from,
        ),
      ),
    );
    _incomingCallId = null;
  }

  bool get _isDesktopPlatform =>
      defaultTargetPlatform == TargetPlatform.linux ||
      defaultTargetPlatform == TargetPlatform.macOS ||
      defaultTargetPlatform == TargetPlatform.windows;

  String _selfActorUrl(CoreConfig config) {
    final base = config.publicBaseUrl.trim().replaceAll(RegExp(r'/+$'), '');
    final user = config.username.trim();
    return '$base/users/$user';
  }

  void _selectSection(int index) {
    if (!mounted || index < 0 || index >= _pages.length) return;
    if (_index == index) return;
    setState(() => _index = index);
    if (!MediaQuery.of(context).disableAnimations) {
      _sectionFade.forward(from: 0.35);
    }
  }

  void _openComposer() {
    if (!mounted) return;
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => ComposeScreen(appState: widget.appState),
      ),
    );
  }

  void _focusSearch() {
    _selectSection(1);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final editable = _findEditableText(_searchScreenKey.currentContext);
      editable?.focusNode.requestFocus();
    });
  }

  EditableText? _findEditableText(BuildContext? root) {
    if (root == null) return null;
    EditableText? found;
    void visit(Element element) {
      if (found != null) return;
      final widget = element.widget;
      if (widget is EditableText) {
        found = widget;
        return;
      }
      element.visitChildren(visit);
    }

    root.visitChildElements(visit);
    return found;
  }

  void _closeTopRoute() {
    final navigator = Navigator.maybeOf(context);
    if (navigator != null && navigator.canPop()) {
      navigator.maybePop();
    }
  }

  Map<ShortcutActivator, VoidCallback> _shortcutBindings() {
    return {
      const SingleActivator(LogicalKeyboardKey.digit1, control: true): () =>
          _selectSection(0),
      const SingleActivator(LogicalKeyboardKey.digit2, control: true): () =>
          _selectSection(1),
      const SingleActivator(LogicalKeyboardKey.digit3, control: true): () =>
          _selectSection(2),
      const SingleActivator(LogicalKeyboardKey.digit4, control: true): () =>
          _selectSection(3),
      const SingleActivator(LogicalKeyboardKey.digit5, control: true): () =>
          _selectSection(4),
      const SingleActivator(LogicalKeyboardKey.digit6, control: true): () =>
          _selectSection(5),
      const SingleActivator(LogicalKeyboardKey.digit7, control: true): () =>
          _selectSection(6),
      const SingleActivator(LogicalKeyboardKey.keyN, control: true):
          _openComposer,
      const SingleActivator(LogicalKeyboardKey.keyF, control: true):
          _focusSearch,
      const SingleActivator(LogicalKeyboardKey.escape): _closeTopRoute,
    };
  }

  Widget _wrapWithShortcuts(Widget child) {
    if (!_isDesktopPlatform) return child;
    return CallbackShortcuts(
      bindings: _shortcutBindings(),
      // The focus node exists only so keyboard events inside the shell have a
      // descendant to start from; it is skipped by Tab traversal.
      child: Focus(
        autofocus: true,
        skipTraversal: true,
        child: child,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final isWide =
        MediaQuery.of(context).size.width >= UiTokens.desktopBreakpoint;
    final unread = widget.appState.unreadNotifications;
    final unreadChats = widget.appState.unreadChats;
    final items = _navItems(context, unread, unreadChats);

    if (!isWide) {
      return _wrapWithShortcuts(
        Scaffold(
          body: AnimatedBuilder(
            animation: _sectionFade,
            builder: (context, child) => Opacity(
              opacity: 0.55 + 0.45 * _sectionFade.value,
              child: child,
            ),
            child: IndexedStack(
              key: const ValueKey('shell-pages'),
              index: _index,
              children: _pages,
            ),
          ),
          bottomNavigationBar: NavigationBar(
            selectedIndex: _index,
            onDestinationSelected: _selectSection,
            destinations: [
              for (final it in items)
                NavigationDestination(
                  tooltip: '${it.label} (${it.shortcut})',
                  icon: it.badge > 0
                      ? Badge(
                          label: Text('${it.badge}'),
                          child: Icon(it.icon),
                        )
                      : Icon(it.icon),
                  label: it.label,
                ),
            ],
          ),
        ),
      );
    }

    return _wrapWithShortcuts(
      Scaffold(
        body: Row(
          children: [
            _FediRail(
              appState: widget.appState,
              items: items,
              index: _index,
              onSelect: _selectSection,
              onBrandTap: _openAnnouncements,
              hasBrandNews: _announcementsUnread,
            ),
            Expanded(
              child: Row(
                children: [
                  Expanded(
                    child: AnimatedBuilder(
                      animation: _sectionFade,
                      builder: (context, child) => Opacity(
                        opacity: 0.55 + 0.45 * _sectionFade.value,
                        child: child,
                      ),
                      child: IndexedStack(
                        key: const ValueKey('shell-pages'),
                        index: _index,
                        children: _pages,
                      ),
                    ),
                  ),
                  SizedBox(
                    width: UiTokens.rightSidebarWidth,
                    child: RightSidebar(appState: widget.appState),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  List<_RailItem> _navItems(BuildContext context, int unread, int unreadChats) {
    final items = <_RailItem>[
      _RailItem(
          icon: Icons.dynamic_feed_rounded,
          label: context.l10n.navTimeline,
          shortcut: 'Ctrl+1'),
      _RailItem(
          icon: Icons.search_rounded,
          label: context.l10n.navSearch,
          shortcut: 'Ctrl+2'),
      _RailItem(
          icon: Icons.forum_rounded,
          label: context.l10n.navChat,
          shortcut: 'Ctrl+3',
          badge: unreadChats),
      _RailItem(
          icon: Icons.notifications_rounded,
          label: context.l10n.navNotifications,
          shortcut: 'Ctrl+4',
          badge: unread),
      _RailItem(
          icon: Icons.hub_rounded,
          label: context.l10n.navRelays,
          shortcut: 'Ctrl+5'),
      _RailItem(
          icon: Icons.tune_rounded,
          label: context.l10n.navSettings,
          shortcut: 'Ctrl+6'),
    ];
    if (_pages.length > items.length) {
      items.add(_RailItem(
          icon: Icons.person_rounded,
          label: context.l10n.settingsAccount,
          shortcut: 'Ctrl+7'));
    }
    return items;
  }

  void _showChatSnack() {
    final messenger = ScaffoldMessenger.maybeOf(context);
    if (messenger == null) return;
    messenger.clearSnackBars();
    messenger.showSnackBar(
      SnackBar(
        content: Text(context.l10n.chatNewMessage),
        action: SnackBarAction(
          label: context.l10n.chatOpen,
          onPressed: () => setState(() => _index = 2),
        ),
      ),
    );
  }
}

class _RailItem {
  const _RailItem({
    required this.icon,
    required this.label,
    required this.shortcut,
    this.badge = 0,
  });

  final IconData icon;
  final String label;
  final String shortcut;
  final int badge;
}

/// Desktop navigation rail with the Aurora visual language: brand mark, animated
/// selection indicator and a compact connectivity status instead of the old
/// technical status bar.
class _FediRail extends StatelessWidget {
  const _FediRail({
    required this.appState,
    required this.items,
    required this.index,
    required this.onSelect,
    required this.onBrandTap,
    required this.hasBrandNews,
  });

  final AppState appState;
  final List<_RailItem> items;
  final int index;
  final ValueChanged<int> onSelect;
  final VoidCallback onBrandTap;
  final bool hasBrandNews;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final running = appState.isRunning;
    return Container(
      width: UiTokens.railWidth,
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        border: Border(
          right: BorderSide(color: scheme.outlineVariant, width: 1),
        ),
      ),
      child: Stack(
        children: [
          Positioned(
            top: -70,
            left: -40,
            right: -40,
            height: 220,
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: RadialGradient(
                    center: Alignment.topCenter,
                    radius: 0.9,
                    colors: [
                      const Color(UiTokens.brandViolet).withAlpha(46),
                      const Color(UiTokens.brandViolet).withAlpha(0),
                    ],
                  ),
                ),
              ),
            ),
          ),
          Positioned(
            bottom: -60,
            left: -30,
            right: -30,
            height: 180,
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: RadialGradient(
                    center: Alignment.bottomCenter,
                    radius: 0.9,
                    colors: [
                      const Color(UiTokens.brandCyan).withAlpha(24),
                      const Color(UiTokens.brandCyan).withAlpha(0),
                    ],
                  ),
                ),
              ),
            ),
          ),
          Positioned.fill(
            child: Column(
              children: [
                const SizedBox(height: UiTokens.gapLg),
                Tooltip(
                  message: 'Fedi3',
                  child: GestureDetector(
                    onTap: onBrandTap,
                    child: Stack(
                      clipBehavior: Clip.none,
                      children: [
                        const BrandMark(size: 46, animated: true),
                        if (hasBrandNews)
                          Positioned(
                            right: -2,
                            top: -2,
                            child: Container(
                              width: 12,
                              height: 12,
                              decoration: BoxDecoration(
                                color: const Color(0xFF22D3EE),
                                shape: BoxShape.circle,
                                border: Border.all(
                                  color: Theme.of(context).colorScheme.surface,
                                  width: 2,
                                ),
                                boxShadow: const [
                                  BoxShadow(
                                    color: Color(0x8822D3EE),
                                    blurRadius: 8,
                                  ),
                                ],
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: UiTokens.gapLg),
                Expanded(
                  child: FocusTraversalGroup(
                    policy: OrderedTraversalPolicy(),
                    child: ListView(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      children: [
                        for (var i = 0; i < items.length; i++)
                          Padding(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 10, vertical: 3),
                            child: FocusTraversalOrder(
                              order: NumericFocusOrder(i.toDouble()),
                              child: _RailButton(
                                item: items[i],
                                active: i == index,
                                onTap: () => onSelect(i),
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.only(bottom: 14, top: 8),
                  child: ValueListenableBuilder<PresenceStatus>(
                    valueListenable: PeerPresenceStore.instance.selfStatus,
                    builder: (context, presence, _) {
                      return PopupMenuButton<PresenceStatus>(
                        tooltip: context.l10n.presenceChangeTooltip,
                        position: PopupMenuPosition.over,
                        offset: const Offset(8, -8),
                        color: const Color(UiTokens.darkSurface),
                        surfaceTintColor: Colors.transparent,
                        elevation: 18,
                        shape: RoundedRectangleBorder(
                          borderRadius:
                              BorderRadius.circular(UiTokens.radiusCardSm),
                          side: BorderSide(
                            color: scheme.primary.withAlpha(55),
                          ),
                        ),
                        constraints: const BoxConstraints(
                          minWidth: 190,
                          maxWidth: 230,
                        ),
                        onSelected: (status) async {
                          await appState.updatePrefs((current) =>
                              current.copyWith(
                                  presenceStatus: presenceStatusName(status)));
                          await PeerPresenceStore.instance
                              .setSelfStatus(status);
                        },
                        itemBuilder: (context) => [
                          for (final status in PresenceStatus.values)
                            PopupMenuItem<PresenceStatus>(
                              value: status,
                              height: 40,
                              padding:
                                  const EdgeInsets.symmetric(horizontal: 14),
                              child: Row(
                                children: [
                                  Container(
                                    width: 8,
                                    height: 8,
                                    decoration: BoxDecoration(
                                      shape: BoxShape.circle,
                                      color: _presenceColor(status),
                                      boxShadow: status == presence
                                          ? [
                                              BoxShadow(
                                                color: _presenceColor(status)
                                                    .withAlpha(120),
                                                blurRadius: 7,
                                              ),
                                            ]
                                          : null,
                                    ),
                                  ),
                                  const SizedBox(width: 10),
                                  Expanded(
                                    child: Text(
                                      _presenceLabel(context, status),
                                      style: TextStyle(
                                        fontSize: 12.5,
                                        fontWeight: status == presence
                                            ? FontWeight.w700
                                            : FontWeight.w500,
                                      ),
                                    ),
                                  ),
                                  if (status == presence)
                                    Icon(
                                      Icons.check_rounded,
                                      size: 15,
                                      color: scheme.primary,
                                    ),
                                ],
                              ),
                            ),
                        ],
                        child: Column(
                          children: [
                            StatusDot(color: _presenceColor(presence), size: 7),
                            const SizedBox(height: 5),
                            Text(
                              _presenceLabel(context, presence),
                              textAlign: TextAlign.center,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: Theme.of(context)
                                  .textTheme
                                  .labelSmall
                                  ?.copyWith(
                                    color: scheme.onSurface,
                                    fontSize: 9.5,
                                    fontWeight: FontWeight.w700,
                                  ),
                            ),
                            const SizedBox(height: 2),
                            Text(
                              running
                                  ? context.l10n.coreOnline
                                  : context.l10n.coreOffline,
                              textAlign: TextAlign.center,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: Theme.of(context)
                                  .textTheme
                                  .labelSmall
                                  ?.copyWith(
                                    color: scheme.onSurfaceVariant,
                                    fontSize: 8.5,
                                  ),
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

String _presenceLabel(BuildContext context, PresenceStatus status) {
  return switch (status) {
    PresenceStatus.online => context.l10n.presenceOnline,
    PresenceStatus.busy => context.l10n.presenceBusy,
    PresenceStatus.afk => context.l10n.presenceAfk,
    PresenceStatus.offline => context.l10n.presenceOffline,
  };
}

Color _presenceColor(PresenceStatus status) {
  return switch (status) {
    PresenceStatus.online => const Color(0xFF3DDC97),
    PresenceStatus.busy => const Color(0xFFFF5A6E),
    PresenceStatus.afk => const Color(0xFFF2A33A),
    PresenceStatus.offline => const Color(0xFF8B93A5),
  };
}

class _RailButton extends StatefulWidget {
  const _RailButton({
    required this.item,
    required this.active,
    required this.onTap,
  });

  final _RailItem item;
  final bool active;
  final VoidCallback onTap;

  @override
  State<_RailButton> createState() => _RailButtonState();
}

class _RailButtonState extends State<_RailButton>
    with TickerProviderStateMixin {
  bool _hover = false;
  bool _focused = false;
  bool _pressed = false;

  late final AnimationController _spin = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 4),
  );
  late final AnimationController _burst = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 650),
  );

  @override
  void initState() {
    super.initState();
    if (widget.active) _spin.repeat();
  }

  @override
  void didUpdateWidget(covariant _RailButton oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.active && !_spin.isAnimating) {
      _spin.repeat();
    } else if (!widget.active && _spin.isAnimating) {
      _spin.stop();
      _spin.value = 0;
    }
  }

  @override
  void dispose() {
    _spin.dispose();
    _burst.dispose();
    super.dispose();
  }

  void _handleTap() {
    _burst.forward(from: 0);
    widget.onTap();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final active = widget.active;
    final animate = !MediaQuery.of(context).disableAnimations;
    final bg = active
        ? scheme.primary.withAlpha(26)
        : _hover
            ? scheme.onSurface.withAlpha(10)
            : Colors.transparent;
    final fg = active
        ? scheme.primary
        : _hover
            ? scheme.onSurface
            : scheme.onSurfaceVariant;
    return AccessibleTooltip(
      message: '${widget.item.label} (${widget.item.shortcut})',
      button: true,
      selected: active,
      child: Focus(
        onFocusChange: (hasFocus) => setState(() => _focused = hasFocus),
        onKeyEvent: (node, event) {
          if (event is KeyDownEvent &&
              (event.logicalKey == LogicalKeyboardKey.enter ||
                  event.logicalKey == LogicalKeyboardKey.space)) {
            _handleTap();
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          onEnter: (_) => setState(() => _hover = true),
          onExit: (_) => setState(() => _hover = false),
          child: GestureDetector(
            onTapDown: (_) => setState(() => _pressed = true),
            onTapCancel: () => setState(() => _pressed = false),
            onTapUp: (_) => setState(() => _pressed = false),
            onTap: _handleTap,
            child: AnimatedScale(
              scale: _pressed ? 0.94 : 1,
              duration: const Duration(milliseconds: 120),
              curve: Curves.easeOut,
              child: ConstrainedBox(
                constraints: const BoxConstraints(minWidth: 48, minHeight: 48),
                child: AnimatedBuilder(
                  animation: Listenable.merge([_spin, _burst]),
                  builder: (context, child) {
                    return AnimatedContainer(
                      duration: UiTokens.motionFast,
                      curve: Curves.easeOutCubic,
                      height: 62,
                      decoration: BoxDecoration(
                        color: bg,
                        borderRadius: BorderRadius.circular(18),
                        boxShadow: active
                            ? [
                                BoxShadow(
                                  color: scheme.primary.withAlpha(45),
                                  blurRadius: 14,
                                  spreadRadius: 1,
                                ),
                              ]
                            : null,
                        border: Border.all(
                          color: active
                              ? Colors.transparent
                              : _focused
                                  ? scheme.primary.withAlpha(140)
                                  : Colors.transparent,
                        ),
                      ),
                      child: Stack(
                        clipBehavior: Clip.none,
                        children: [
                          Positioned.fill(
                            child: IgnorePointer(
                              child: CustomPaint(
                                painter: _RailEffectsPainter(
                                  spin: animate ? _spin.value : 0,
                                  burst: animate ? _burst.value : 0,
                                  active: active,
                                  colorA: const Color(UiTokens.brandViolet),
                                  colorB: const Color(UiTokens.brandCyan),
                                ),
                              ),
                            ),
                          ),
                          Center(
                            child: Column(
                              mainAxisAlignment: MainAxisAlignment.center,
                              children: [
                                Stack(
                                  clipBehavior: Clip.none,
                                  children: [
                                    Icon(widget.item.icon, size: 22, color: fg),
                                    if (widget.item.badge > 0)
                                      Positioned(
                                        right: -9,
                                        top: -7,
                                        child: Container(
                                          padding: const EdgeInsets.symmetric(
                                              horizontal: 5, vertical: 1),
                                          decoration: BoxDecoration(
                                            color: scheme.error,
                                            borderRadius:
                                                BorderRadius.circular(99),
                                          ),
                                          child: Text(
                                            widget.item.badge > 99
                                                ? '99+'
                                                : '${widget.item.badge}',
                                            style: const TextStyle(
                                              color: Colors.white,
                                              fontSize: 9,
                                              fontWeight: FontWeight.w800,
                                            ),
                                          ),
                                        ),
                                      ),
                                  ],
                                ),
                                const SizedBox(height: 4),
                                Text(
                                  widget.item.label,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: Theme.of(context)
                                      .textTheme
                                      .labelSmall
                                      ?.copyWith(
                                        color: fg,
                                        fontSize: 9.5,
                                        fontWeight: active
                                            ? FontWeight.w800
                                            : FontWeight.w600,
                                      ),
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                    );
                  },
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _RailEffectsPainter extends CustomPainter {
  const _RailEffectsPainter({
    required this.spin,
    required this.burst,
    required this.active,
    required this.colorA,
    required this.colorB,
  });

  final double spin;
  final double burst;
  final bool active;
  final Color colorA;
  final Color colorB;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final center = rect.center;
    final rrect = RRect.fromRectAndRadius(
      rect.deflate(1.4),
      const Radius.circular(18),
    );

    if (active) {
      final shader = SweepGradient(
        colors: [colorA, colorB, colorA, colorB, colorA],
        transform: GradientRotation(spin * 2 * math.pi),
      ).createShader(rect);
      canvas.drawRRect(
        rrect,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 3
          ..shader = shader
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4),
      );
      canvas.drawRRect(
        rrect,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.2
          ..shader = shader,
      );

      const particles = 8;
      for (var i = 0; i < particles; i++) {
        final phase = spin * 2 * math.pi + i * 2 * math.pi / particles;
        final radius =
            size.shortestSide * 0.62 + math.sin(spin * 4 * math.pi + i) * 2.2;
        final pos =
            center + Offset(math.cos(phase) * radius, math.sin(phase) * radius);
        final alpha =
            (0.30 + 0.25 * math.sin(spin * 6 * math.pi + i)).clamp(0.0, 1.0);
        final color = i.isEven ? colorB : colorA;
        canvas.drawCircle(
          pos,
          1.2 + 0.7 * math.sin(spin * 3 * math.pi + i).abs(),
          Paint()..color = color.withAlpha((alpha * 255).round()),
        );
      }
    }

    if (burst > 0 && burst < 1) {
      const sparks = 14;
      final fade = 1 - burst;
      for (var i = 0; i < sparks; i++) {
        final angle = i * 2 * math.pi / sparks;
        final distance = size.shortestSide * (0.35 + 0.75 * burst);
        final pos = center +
            Offset(math.cos(angle) * distance, math.sin(angle) * distance);
        final color = i.isEven ? colorB : colorA;
        canvas.drawCircle(
          pos,
          0.8 + 2.2 * fade,
          Paint()..color = color.withAlpha((fade * 220).round()),
        );
      }
    }
  }

  @override
  bool shouldRepaint(covariant _RailEffectsPainter oldDelegate) {
    return oldDelegate.spin != spin ||
        oldDelegate.burst != burst ||
        oldDelegate.active != active;
  }
}
