/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/material.dart';

import '../state/app_state.dart';
import '../l10n/l10n_ext.dart';
import '../model/core_config.dart';
import '../services/core_event_stream.dart';
import '../services/actor_repository.dart';
import '../services/notification_service.dart';
import '../services/update_service.dart';
import 'screens/notifications_screen.dart';
import 'screens/relays_screen.dart';
import 'screens/search_screen.dart';
import 'screens/settings_screen.dart';
import 'screens/timelines_screen.dart';
import 'screens/chat_screen.dart';
import 'widgets/update_banner.dart';
import 'widgets/nerd_status_bar.dart';
import 'widgets/right_sidebar.dart';
import 'theme/ui_tokens.dart';

class Shell extends StatefulWidget {
  const Shell({super.key, required this.appState});

  final AppState appState;

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  static const Set<String> _directNotifTypes = {
    'Create',
    'Announce',
  };
  int _index = 0;
  late final List<Widget> _pages;
  StreamSubscription<CoreEvent>? _chatStream;
  Timer? _chatDebounce;
  Timer? _chatRetry;
  CoreConfig? _chatStreamConfig;
  StreamSubscription<CoreEvent>? _notifStream;
  Timer? _notifDebounce;
  Timer? _notifRetry;
  CoreConfig? _notifStreamConfig;
  StreamSubscription<CoreEvent>? _profileStream;
  Timer? _profileRetry;
  CoreConfig? _profileStreamConfig;
  late bool _lastRunning;
  late final VoidCallback _appStateListener;

  @override
  void initState() {
    super.initState();
    _pages = <Widget>[
      TimelinesScreen(appState: widget.appState),
      SearchScreen(appState: widget.appState),
      ChatScreen(appState: widget.appState),
      NotificationsScreen(appState: widget.appState),
      RelaysScreen(appState: widget.appState),
      SettingsScreen(appState: widget.appState),
    ];
    _lastRunning = widget.appState.isRunning;
    _appStateListener = () {
      final running = widget.appState.isRunning;
      final cfg = widget.appState.config;
      final configChanged = !identical(_chatStreamConfig, cfg) ||
          !identical(_notifStreamConfig, cfg) ||
          !identical(_profileStreamConfig, cfg);
      if (!running) {
        _stopChatStream();
        _stopNotifStream();
        _stopProfileStream();
      } else if (running && (!_lastRunning || configChanged)) {
        _startChatStream();
        _startNotifStream();
        _startProfileStream();
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);
    if (widget.appState.isRunning) {
      _startChatStream();
      _startNotifStream();
      _startProfileStream();
    }
    UpdateService.instance.start();
  }

  @override
  void dispose() {
    _chatStream?.cancel();
    _chatDebounce?.cancel();
    _chatRetry?.cancel();
    _notifStream?.cancel();
    _notifDebounce?.cancel();
    _notifRetry?.cancel();
    _profileStream?.cancel();
    _profileRetry?.cancel();
    UpdateService.instance.stop();
    widget.appState.removeListener(_appStateListener);
    super.dispose();
  }

  void _startChatStream() {
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_chatStreamConfig, cfg) && _chatStream != null) return;
    _chatStreamConfig = cfg;
    _chatStream?.cancel();
    _chatStream = CoreEventStream(config: cfg).stream(kind: 'chat').listen((ev) {
      if (!mounted) return;
      if (ev.kind != 'chat') return;
      final lastSeen = widget.appState.prefs.lastChatSeenMs;
      if (ev.tsMs <= lastSeen) return;
      _chatDebounce?.cancel();
      _chatDebounce = Timer(const Duration(milliseconds: 350), () {
        if (!mounted) return;
        final onChatTab = _index == 2;
        if (!onChatTab) {
          widget.appState.incrementUnreadChats();
          _showChatSnack();
          if (widget.appState.prefs.notifyChat && !_notificationsMuted()) {
            NotificationService.showChatNotification(
              title: context.l10n.chatTitle,
              body: context.l10n.chatNewMessageBody,
            );
          }
        }
      });
    }, onError: (_) => _scheduleChatRetry(), onDone: _scheduleChatRetry);
  }

  void _stopChatStream() {
    _chatRetry?.cancel();
    _chatStream?.cancel();
    _chatStream = null;
  }

  void _startNotifStream() {
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_notifStreamConfig, cfg) && _notifStream != null) return;
    _notifStreamConfig = cfg;
    _notifStream?.cancel();
    _notifStream = CoreEventStream(config: cfg).stream().listen((ev) {
      if (!mounted) return;
      if (ev.kind != 'notification' && ev.kind != 'inbox') return;
      if (!_isDirectInteraction(ev)) return;
      final lastSeen = widget.appState.prefs.lastNotificationsSeenMs;
      if (ev.tsMs <= lastSeen) return;
      _notifDebounce?.cancel();
      _notifDebounce = Timer(const Duration(milliseconds: 350), () {
        if (!mounted) return;
        final onNotifTab = _index == 3;
        if (!onNotifTab) {
          widget.appState.incrementUnreadNotifications();
          if (widget.appState.prefs.notifyDirect && !_notificationsMuted()) {
            NotificationService.showGeneralNotification(
              title: context.l10n.notificationsTitle,
              body: context.l10n.notificationsNewActivity,
            );
          }
        }
      });
    }, onError: (_) => _scheduleNotifRetry(), onDone: _scheduleNotifRetry);
  }

  void _stopNotifStream() {
    _notifRetry?.cancel();
    _notifStream?.cancel();
    _notifStream = null;
  }

  void _startProfileStream() {
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_profileStreamConfig, cfg) && _profileStream != null) return;
    _profileStreamConfig = cfg;
    _profileStream?.cancel();
    _profileStream = CoreEventStream(config: cfg).stream(kind: 'profile').listen((ev) {
      if (ev.kind != 'profile' || ev.activityType != 'featured') return;
      final base = cfg.publicBaseUrl.trim().replaceAll(RegExp(r'/$'), '');
      final actorUrl = '$base/users/${cfg.username}';
      ActorRepository.instance.refreshActor(actorUrl);
    }, onError: (_) => _scheduleProfileRetry(), onDone: _scheduleProfileRetry);
  }

  void _stopProfileStream() {
    _profileRetry?.cancel();
    _profileStream?.cancel();
    _profileStream = null;
  }

  void _scheduleNotifRetry() {
    if (!mounted) return;
    _notifStream = null;
    if (!widget.appState.isRunning) return;
    _notifRetry?.cancel();
    _notifRetry = Timer(const Duration(seconds: 2), () {
      if (!mounted) return;
      _startNotifStream();
    });
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

  void _scheduleProfileRetry() {
    if (!mounted) return;
    _profileStream = null;
    if (!widget.appState.isRunning) return;
    _profileRetry?.cancel();
    _profileRetry = Timer(const Duration(seconds: 2), () {
      if (!mounted) return;
      _startProfileStream();
    });
  }

  bool _isDirectInteraction(CoreEvent ev) {
    if (ev.kind != 'inbox') return false;
    if (!widget.appState.prefs.notifyDirect) return false;
    final ty = ev.activityType?.trim() ?? '';
    if (ty.isEmpty) return false;
    return _directNotifTypes.contains(ty);
  }

  bool _notificationsMuted() {
    final until = widget.appState.prefs.notifyMutedUntilMs;
    if (until <= 0) return false;
    return DateTime.now().millisecondsSinceEpoch < until;
  }

  @override
  Widget build(BuildContext context) {
    final isWide = MediaQuery.of(context).size.width >= UiTokens.desktopBreakpoint;
    final unread = widget.appState.unreadNotifications;
    final unreadChats = widget.appState.unreadChats;

    if (!isWide) {
      return Scaffold(
        body: Column(
          children: [
            const UpdateBanner(),
            Expanded(child: IndexedStack(index: _index, children: _pages)),
          ],
        ),
        bottomNavigationBar: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            SafeArea(top: false, child: NerdStatusBar(appState: widget.appState)),
            NavigationBar(
              selectedIndex: _index,
              onDestinationSelected: (i) => setState(() => _index = i),
              destinations: [
                NavigationDestination(icon: const Icon(Icons.dynamic_feed), label: context.l10n.navTimeline),
                NavigationDestination(icon: const Icon(Icons.search), label: context.l10n.navSearch),
                NavigationDestination(icon: _badge(Icons.forum, unreadChats), label: context.l10n.navChat),
                NavigationDestination(icon: _badge(Icons.notifications, unread), label: context.l10n.navNotifications),
                NavigationDestination(icon: const Icon(Icons.hub), label: context.l10n.navRelays),
                NavigationDestination(icon: const Icon(Icons.settings), label: context.l10n.navSettings),
              ],
            ),
          ],
        ),
      );
    }

    return Scaffold(
      body: Row(
        children: [
          NavigationRail(
            selectedIndex: _index,
            onDestinationSelected: (i) => setState(() => _index = i),
            labelType: NavigationRailLabelType.all,
            destinations: [
              NavigationRailDestination(icon: const Icon(Icons.dynamic_feed), label: Text(context.l10n.navTimeline)),
              NavigationRailDestination(icon: const Icon(Icons.search), label: Text(context.l10n.navSearch)),
              NavigationRailDestination(icon: _badge(Icons.forum, unreadChats), label: Text(context.l10n.navChat)),
              NavigationRailDestination(icon: _badge(Icons.notifications, unread), label: Text(context.l10n.navNotifications)),
              NavigationRailDestination(icon: const Icon(Icons.hub), label: Text(context.l10n.navRelays)),
              NavigationRailDestination(icon: const Icon(Icons.settings), label: Text(context.l10n.navSettings)),
            ],
          ),
          const VerticalDivider(width: 1),
          Expanded(
            child: Row(
                children: [
                  Expanded(
                    child: Column(
                      children: [
                        const UpdateBanner(),
                        Expanded(child: IndexedStack(index: _index, children: _pages)),
                        SafeArea(top: false, child: NerdStatusBar(appState: widget.appState)),
                      ],
                    ),
                  ),
                const VerticalDivider(width: 1),
                SizedBox(
                  width: UiTokens.rightSidebarWidth,
                  child: RightSidebar(appState: widget.appState),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _badge(IconData icon, int count) {
    if (count <= 0) return Icon(icon);
    return Badge(
      label: Text('$count'),
      child: Icon(icon),
    );
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
