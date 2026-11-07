import 'dart:async';
import 'package:flutter/widgets.dart';

/// Shared foreground gate; active calls keep their dedicated signaling timers.
class AppActivity {
  static bool foreground = true;
  static final _changes = StreamController<bool>.broadcast(sync: true);
  static Stream<bool> get changes => _changes.stream;
  static void update(AppLifecycleState state) {
    if (state == AppLifecycleState.inactive) return;
    final next = state == AppLifecycleState.resumed;
    if (next == foreground) return;
    foreground = next;
    _changes.add(next);
  }

  static Future<void> waitForForeground() async {
    if (!foreground) await changes.firstWhere((value) => value);
  }

  static Timer periodic(Duration duration, void Function(Timer) callback) =>
      Timer.periodic(duration, (timer) {
        if (foreground) callback(timer);
      });
}
