import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core/core_api.dart';
import '../model/chat_models.dart';

/// One foreground dialog per invite, even when both shell and thread see it.
class IncomingCallGate {
  static final Map<String, DateTime> _shown = {};
  static bool claim(String id) {
    final now = DateTime.now();
    _shown.removeWhere((_, at) => now.difference(at).inMinutes >= 2);
    if (_shown.containsKey(id)) return false;
    _shown[id] = now;
    return true;
  }
}

/// The Android service has no shell. Polling also covers invites delivered
/// during the handoff between the visible engine and the background engine.
class IncomingCallNotifications {
  IncomingCallNotifications(
      {required this.api,
      required this.selfActor,
      required this.onInvite,
      required this.onEnded})
      : _afterMs = DateTime.now().millisecondsSinceEpoch - 60000;

  final CoreApi api;
  final String selfActor;
  final Future<void> Function(ChatCallSignal) onInvite;
  final Future<void> Function() onEnded;
  final int _afterMs;
  int _afterId = 0;
  String? _ringing;
  Timer? _timer;
  bool _polling = false;
  bool _stopped = false;

  void start() {
    _timer ??= Timer.periodic(const Duration(seconds: 2), (_) => poll());
    unawaited(poll());
  }

  void stop() {
    _stopped = true;
    _timer?.cancel();
  }

  Future<void> poll() async {
    if (_polling || _stopped) return;
    _polling = true;
    try {
      final signals = await api.fetchGlobalChatCallSignals(
          afterId: _afterId, afterMs: _afterMs, limit: 200);
      if (_stopped) return;
      final ended = signals
          .where((s) => const ['leave', 'reject', 'decline'].contains(s.action))
          .map((s) => s.callId)
          .toSet();
      if (_ringing != null && ended.contains(_ringing)) {
        _ringing = null;
        await onEnded();
      }
      ChatCallSignal? newest;
      for (final signal in signals) {
        if (signal.id > _afterId) _afterId = signal.id;
        if (signal.action != 'invite' ||
            ended.contains(signal.callId) ||
            signal.fromActor.trim().replaceFirst(RegExp(r'/$'), '') ==
                selfActor.trim().replaceFirst(RegExp(r'/$'), '') ||
            signal.callId == _ringing ||
            DateTime.now().millisecondsSinceEpoch - signal.createdAtMs >
                60000) {
          continue;
        }
        if (newest == null || signal.id > newest.id) newest = signal;
      }
      if (newest != null) {
        debugPrint('[bg] ring ${newest.callId} from ${newest.fromActor}');
        _ringing = newest.callId;
        await onInvite(newest);
      }
    } catch (e) {
      debugPrint('[bg] call poll failed: $e');
      /* Retry transient local-core failures on the next tick. */
    } finally {
      _polling = false;
    }
  }
}
