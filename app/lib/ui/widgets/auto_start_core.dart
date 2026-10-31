/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:flutter/material.dart';

import '../../state/app_state.dart';
import '../../state/peer_presence_store.dart';

class AutoStartCore extends StatefulWidget {
  const AutoStartCore({super.key, required this.appState, required this.child});

  final AppState appState;
  final Widget child;

  @override
  State<AutoStartCore> createState() => _AutoStartCoreState();
}

class _AutoStartCoreState extends State<AutoStartCore> {
  bool _started = false;
  bool _wasRunning = false;

  @override
  void initState() {
    super.initState();
    widget.appState.addListener(_onStateChanged);
    _wasRunning = widget.appState.isRunning;
    _kick();
  }

  @override
  void didUpdateWidget(covariant AutoStartCore oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.appState != widget.appState) {
      oldWidget.appState.removeListener(_onStateChanged);
      widget.appState.addListener(_onStateChanged);
      _started = false;
      _wasRunning = widget.appState.isRunning;
      _kick();
    } else {
      _kick();
    }
  }

  @override
  void dispose() {
    widget.appState.removeListener(_onStateChanged);
    super.dispose();
  }

  void _onStateChanged() {
    final running = widget.appState.isRunning;
    if (_wasRunning && !running) {
      // H-51/P0-12: the core died (crash, stop from another screen, watchdog).
      // Reset the latch so the auto-start can run again.
      _started = false;
    }
    _wasRunning = running;
    if (!running && !_started) {
      _kick();
    }
  }

  void _kick() {
    final cfg = widget.appState.config;
    if (cfg != null) {
      PeerPresenceStore.instance.start(cfg);
    }
    if (_started) return;
    if (cfg == null) return;
    if (widget.appState.isRunning) {
      _started = true;
      _wasRunning = true;
      return;
    }
    _started = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      widget.appState.startCore();
    });
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
