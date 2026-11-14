/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:material_ui/material_ui.dart';

import '../../core/fedi3_core.dart' show CorePowerMode;
import '../../services/app_activity.dart';
import '../../services/identity_sync.dart';
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
  Timer? _accountTimer;
  bool _started = false;
  bool _wasRunning = false;
  Object? _attemptedConfig;

  @override
  void initState() {
    super.initState();
    widget.appState.addListener(_onStateChanged);
    _wasRunning = widget.appState.isRunning;
    _kick();
    _accountTimer = Timer.periodic(const Duration(seconds: 5), (_) {
      if (AppActivity.foreground) {
        unawaited(widget.appState.refreshAccountProfile());
      }
    });
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
    _accountTimer?.cancel();
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

  /// A core started while the UI is visible must use the foreground cadence,
  /// even when the previous owner (the Android keep-alive isolate) left it in
  /// background mode.
  void _applyPowerMode() {
    widget.appState.setPowerMode(
      AppActivity.foreground
          ? CorePowerMode.foreground
          : CorePowerMode.background,
    );
  }

  void _kick() {
    final cfg = widget.appState.config;
    if (!identical(cfg, _attemptedConfig)) _started = false;
    if (cfg != null) {
      PeerPresenceStore.instance
          .start(cfg, selfStatus: widget.appState.prefs.presenceStatus);
    }
    if (_started) return;
    if (cfg == null) return;
    if (identical(cfg, _attemptedConfig) && widget.appState.lastError != null) {
      return;
    }
    if (widget.appState.isRunning) {
      _started = true;
      _wasRunning = true;
      _applyPowerMode();
      return;
    }
    _started = true;
    _attemptedConfig = cfg;
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      if (!mounted) return;
      await widget.appState.startCore();
      if (mounted) _applyPowerMode();
      if (mounted && !widget.appState.isRunning) _started = false;
      final running = widget.appState.config;
      if (!mounted || !widget.appState.isRunning || running == null) return;
      // Every existing client may own a legacy token-encrypted blob, including
      // clients imported through QR. Migrate using the unchanged local identity.
      // Existing v2 packages are left intact; publishing never blocks the UI.
      unawaited(IdentitySync.ensurePublished(running));
    });
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
