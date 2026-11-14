import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:fedi3/services/app_activity.dart';

void main() {
  test(
      'hidden desktops retain their core and Android delegates only without a call',
      () {
    AppActivity.update(AppLifecycleState.hidden);
    for (final platform in [
      TargetPlatform.linux,
      TargetPlatform.windows,
      TargetPlatform.macOS,
      TargetPlatform.iOS
    ]) {
      expect(AppActivity.shouldStopCore(platform: platform, coreHeld: false),
          isFalse);
    }
    expect(
        AppActivity.shouldStopCore(
            platform: TargetPlatform.android, coreHeld: false),
        isTrue);
    expect(
        AppActivity.shouldStopCore(
            platform: TargetPlatform.android, coreHeld: true),
        isFalse);
    AppActivity.update(AppLifecycleState.resumed);
    expect(
        AppActivity.shouldStopCore(
            platform: TargetPlatform.android, coreHeld: false),
        isFalse);
  });
}
