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
import '../services/update_service.dart';
import '../services/rss_feed_service.dart';
import 'screens/notifications_screen.dart';
import 'screens/search_screen.dart';
import 'screens/settings_screen.dart';
import 'screens/timelines_screen.dart';
import 'screens/chat_screen.dart';
import 'widgets/client_version_badge.dart';
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
  int _index = 0;
  late final List<Widget> _pages;
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
      SettingsScreen(appState: widget.appState),
    ];
    _lastRunning = widget.appState.isRunning;
    _appStateListener = () {
      final running = widget.appState.isRunning;
      final cfg = widget.appState.config;
      final configChanged = !identical(_profileStreamConfig, cfg);
      if (!running) {
        _stopProfileStream();
      } else if (running && (!_lastRunning || configChanged)) {
        _startProfileStream();
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);
    if (widget.appState.isRunning) {
      _startProfileStream();
    }
    UpdateService.instance.start();
    unawaited(RssFeedService.instance.start());
  }

  @override
  void dispose() {
    _profileStream?.cancel();
    _profileRetry?.cancel();
    UpdateService.instance.stop();
    RssFeedService.instance.stop();
    widget.appState.removeListener(_appStateListener);
    super.dispose();
  }

  void _startProfileStream() {
    if (!widget.appState.isRunning) return;
    final cfg = widget.appState.config;
    if (cfg == null) return;
    if (identical(_profileStreamConfig, cfg) && _profileStream != null) return;
    _profileStreamConfig = cfg;
    _profileStream?.cancel();
    _profileStream =
        CoreEventStream(config: cfg).stream(kind: 'profile').listen((ev) {
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

  @override
  Widget build(BuildContext context) {
    final isWide =
        MediaQuery.of(context).size.width >= UiTokens.desktopBreakpoint;
    final unread = widget.appState.unreadNotifications;
    final unreadChats = widget.appState.unreadChats;

    if (!isWide) {
      return Scaffold(
        body: Stack(
          children: [
            IndexedStack(index: _index, children: _pages),
            const Positioned(
              right: 12,
              bottom: 12,
              child: ClientVersionBadge(),
            ),
          ],
        ),
        bottomNavigationBar: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            SafeArea(
                top: false, child: NerdStatusBar(appState: widget.appState)),
            NavigationBar(
              selectedIndex: _index,
              onDestinationSelected: (i) => setState(() => _index = i),
              destinations: [
                NavigationDestination(
                    icon: const Icon(Icons.dynamic_feed),
                    label: context.l10n.navTimeline),
                NavigationDestination(
                    icon: const Icon(Icons.search),
                    label: context.l10n.navSearch),
                NavigationDestination(
                    icon: _badge(Icons.forum, unreadChats),
                    label: context.l10n.navChat),
                NavigationDestination(
                    icon: _badge(Icons.notifications, unread),
                    label: context.l10n.navNotifications),
                NavigationDestination(
                    icon: const Icon(Icons.settings),
                    label: context.l10n.navSettings),
              ],
            ),
          ],
        ),
      );
    }

    return Scaffold(
      body: Stack(
        children: [
          Row(
            children: [
              NavigationRail(
                selectedIndex: _index,
                onDestinationSelected: (i) => setState(() => _index = i),
                labelType: NavigationRailLabelType.all,
                destinations: [
                  NavigationRailDestination(
                      icon: const Icon(Icons.dynamic_feed),
                      label: Text(context.l10n.navTimeline)),
                  NavigationRailDestination(
                      icon: const Icon(Icons.search),
                      label: Text(context.l10n.navSearch)),
                  NavigationRailDestination(
                      icon: _badge(Icons.forum, unreadChats),
                      label: Text(context.l10n.navChat)),
                  NavigationRailDestination(
                      icon: _badge(Icons.notifications, unread),
                      label: Text(context.l10n.navNotifications)),
                  NavigationRailDestination(
                      icon: const Icon(Icons.settings),
                      label: Text(context.l10n.navSettings)),
                ],
              ),
              const VerticalDivider(width: 1),
              Expanded(
                child: Row(
                  children: [
                    Expanded(
                      child: Column(
                        children: [
                          Expanded(
                              child: IndexedStack(
                                  index: _index, children: _pages)),
                          SafeArea(
                              top: false,
                              child: NerdStatusBar(appState: widget.appState)),
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
          const Positioned(
            right: 12,
            bottom: 12,
            child: ClientVersionBadge(),
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
}
