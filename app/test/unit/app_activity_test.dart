import 'dart:async';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:fedi3/services/app_activity.dart';

void main() {
  testWidgets(
      'background suspends polling and resume releases foreground waiters',
      (tester) async {
    AppActivity.update(AppLifecycleState.resumed);
    var count = 0;
    final timer =
        AppActivity.periodic(const Duration(seconds: 1), (_) => count++);
    await tester.pump(const Duration(seconds: 1));
    expect(count, 1);
    AppActivity.update(AppLifecycleState.paused);
    var resumed = false;
    unawaited(AppActivity.waitForForeground().then((_) => resumed = true));
    await tester.pump(const Duration(seconds: 4));
    expect(count, 1);
    expect(resumed, isFalse);
    AppActivity.update(AppLifecycleState.resumed);
    await tester.pump(const Duration(seconds: 1));
    expect(count, 2);
    expect(resumed, isTrue);
    timer.cancel();
  });
}
