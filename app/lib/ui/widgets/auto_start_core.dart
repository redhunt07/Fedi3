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

  @override
  void initState() {
    super.initState();
    _kick();
  }

  @override
  void didUpdateWidget(covariant AutoStartCore oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.appState != widget.appState) {
      _started = false;
      _kick();
    } else {
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
      return;
    }
    _started = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      widget.appState.startCore();
    });
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
