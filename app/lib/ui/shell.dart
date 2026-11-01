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
import '../services/notification_service.dart';
import 'screens/notifications_screen.dart';
import 'screens/relays_screen.dart';
import 'screens/search_screen.dart';
import 'screens/settings_screen.dart';
import 'screens/timelines_screen.dart';
import 'screens/chat_screen.dart';
import 'widgets/fedi3_ui.dart';
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
  StreamSubscription<CoreEvent>? _chatStream;
  Timer? _chatDebounce;
  Timer? _chatRetry;
  CoreConfig? _chatStreamConfig;
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
      final configChanged = !identical(_chatStreamConfig, cfg);
      if (!running) {
        _stopChatStream();
      } else if (running && (!_lastRunning || configChanged)) {
        _startChatStream();
      }
      _lastRunning = running;
    };
    widget.appState.addListener(_appStateListener);
    if (widget.appState.isRunning) {
      _startChatStream();
    }
  }

  @override
  void dispose() {
    _chatStream?.cancel();
    _chatDebounce?.cancel();
    _chatRetry?.cancel();
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
    _chatStream =
        CoreEventStream(config: cfg).stream(kind: 'chat').listen((ev) {
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
          NotificationService.showChatNotification(
            title: context.l10n.chatTitle,
            body: context.l10n.chatNewMessageBody,
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

  @override
  Widget build(BuildContext context) {
    final isWide =
        MediaQuery.of(context).size.width >= UiTokens.desktopBreakpoint;
    final unread = widget.appState.unreadNotifications;
    final unreadChats = widget.appState.unreadChats;
    final items = _navItems(context, unread, unreadChats);

    if (!isWide) {
      return Scaffold(
        body: IndexedStack(index: _index, children: _pages),
        bottomNavigationBar: NavigationBar(
          selectedIndex: _index,
          onDestinationSelected: (i) => setState(() => _index = i),
          destinations: [
            for (final it in items)
              NavigationDestination(
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
      );
    }

    return Scaffold(
      body: Row(
        children: [
          _FediRail(
            appState: widget.appState,
            items: items,
            index: _index,
            onSelect: (i) => setState(() => _index = i),
          ),
          Expanded(
            child: Row(
              children: [
                Expanded(
                  child: IndexedStack(index: _index, children: _pages),
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
    );
  }

  List<_RailItem> _navItems(
      BuildContext context, int unread, int unreadChats) {
    return [
      _RailItem(
          icon: Icons.dynamic_feed_rounded,
          label: context.l10n.navTimeline),
      _RailItem(icon: Icons.search_rounded, label: context.l10n.navSearch),
      _RailItem(
          icon: Icons.forum_rounded,
          label: context.l10n.navChat,
          badge: unreadChats),
      _RailItem(
          icon: Icons.notifications_rounded,
          label: context.l10n.navNotifications,
          badge: unread),
      _RailItem(icon: Icons.hub_rounded, label: context.l10n.navRelays),
      _RailItem(icon: Icons.tune_rounded, label: context.l10n.navSettings),
    ];
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
    this.badge = 0,
  });

  final IconData icon;
  final String label;
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
  });

  final AppState appState;
  final List<_RailItem> items;
  final int index;
  final ValueChanged<int> onSelect;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final running = appState.isRunning;
    final statusColor =
        running ? const Color(0xFF3DDC97) : const Color(0xFFF2A33A);
    return Container(
      width: UiTokens.railWidth,
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        border: Border(
          right: BorderSide(color: scheme.outlineVariant, width: 1),
        ),
      ),
      child: Column(
        children: [
          const SizedBox(height: UiTokens.gapLg),
          const Tooltip(
            message: 'Fedi3',
            child: BrandMark(size: 46),
          ),
          const SizedBox(height: UiTokens.gapLg),
          Expanded(
            child: ListView(
              padding: const EdgeInsets.symmetric(vertical: 4),
              children: [
                for (var i = 0; i < items.length; i++)
                  Padding(
                    padding: const EdgeInsets.symmetric(
                        horizontal: 10, vertical: 3),
                    child: _RailButton(
                      item: items[i],
                      active: i == index,
                      onTap: () => onSelect(i),
                    ),
                  ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.only(bottom: 18, top: 8),
            child: Column(
              children: [
                StatusDot(color: statusColor, size: 7),
                const SizedBox(height: 6),
                Text(
                  running
                      ? context.l10n.coreOnline
                      : context.l10n.coreOffline,
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: scheme.onSurfaceVariant,
                        fontSize: 9,
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

class _RailButtonState extends State<_RailButton> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final active = widget.active;
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
    return Tooltip(
      message: widget.item.label,
      waitDuration: const Duration(milliseconds: 450),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onTap: widget.onTap,
          child: AnimatedContainer(
            duration: UiTokens.motionFast,
            curve: Curves.easeOutCubic,
            height: 62,
            decoration: BoxDecoration(
              color: bg,
              borderRadius: BorderRadius.circular(18),
              border: Border.all(
                color: active
                    ? scheme.primary.withAlpha(70)
                    : Colors.transparent,
              ),
            ),
            child: Stack(
              alignment: Alignment.center,
              children: [
                if (active)
                  Positioned(
                    left: 2,
                    child: AnimatedContainer(
                      duration: UiTokens.motionBase,
                      width: 3,
                      height: 26,
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(4),
                        gradient: const LinearGradient(
                          begin: Alignment.topCenter,
                          end: Alignment.bottomCenter,
                          colors: [
                            Color(UiTokens.brandViolet),
                            Color(UiTokens.brandCyan),
                          ],
                        ),
                      ),
                    ),
                  ),
                Column(
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
                                borderRadius: BorderRadius.circular(99),
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
                      style: Theme.of(context).textTheme.labelSmall?.copyWith(
                            color: fg,
                            fontSize: 9.5,
                            fontWeight:
                                active ? FontWeight.w800 : FontWeight.w600,
                          ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
