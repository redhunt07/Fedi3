/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// AL-03..06: static verification of the platform configuration (the bash
/// script lives in `scripts/`, which is owned by the Lead; these tests make the
/// same guarantees runnable from `flutter test`).
void main() {
  group('Android manifest (AL-03)', () {
    test(
        'main manifest declares INTERNET, POST_NOTIFICATIONS, network config and allowBackup=false',
        () {
      final manifest =
          File('android/app/src/main/AndroidManifest.xml').readAsStringSync();
      expect(manifest, contains('android.permission.INTERNET'));
      expect(manifest, contains('android.permission.POST_NOTIFICATIONS'));
      expect(
          manifest,
          contains(
              'android:networkSecurityConfig="@xml/network_security_config"'));
      expect(manifest, contains('android:allowBackup="false"'));
    });

    test('debug/profile manifests do not duplicate INTERNET', () {
      for (final path in [
        'android/app/src/debug/AndroidManifest.xml',
        'android/app/src/profile/AndroidManifest.xml',
      ]) {
        final manifest = File(path).readAsStringSync();
        expect(manifest, isNot(contains('android.permission.INTERNET')),
            reason: path);
      }
    });

    test('network_security_config only permits cleartext for loopback', () {
      final config =
          File('android/app/src/main/res/xml/network_security_config.xml')
              .readAsStringSync();
      expect(
          config, contains('<base-config cleartextTrafficPermitted="false">'));
      expect(config, contains('127.0.0.1'));
      expect(config, contains('localhost'));
      expect(config, contains('cleartextTrafficPermitted="true"'));
    });

    test(
        'applicationId/namespace are not com.example and release signing uses key.properties',
        () {
      final gradle = File('android/app/build.gradle.kts').readAsStringSync();
      expect(gradle, contains('applicationId = "org.fedi3.client"'));
      expect(gradle, contains('namespace = "org.fedi3.client"'));
      expect(gradle, isNot(contains('com.example')));
      expect(gradle, contains('key.properties'));
      expect(gradle, contains('signingConfigs.getByName("release")'));
    });
  });

  group('Apple platform config (AL-04..05)', () {
    test(
        'macOS entitlements grant network client/server and user-selected files',
        () {
      for (final path in [
        'macos/Runner/Release.entitlements',
        'macos/Runner/DebugProfile.entitlements',
      ]) {
        final text = File(path).readAsStringSync();
        expect(text, contains('com.apple.security.network.client'),
            reason: path);
        expect(text, contains('com.apple.security.network.server'),
            reason: path);
        expect(
            text, contains('com.apple.security.files.user-selected.read-write'),
            reason: path);
      }
    });

    test('iOS Info.plist has ATS local networking and usage descriptions', () {
      final plist = File('ios/Runner/Info.plist').readAsStringSync();
      expect(plist, contains('NSAppTransportSecurity'));
      expect(plist, contains('NSAllowsLocalNetworking'));
      expect(plist, contains('NSPhotoLibraryUsageDescription'));
      expect(plist, contains('NSCameraUsageDescription'));
    });

    test('Linux application id matches the release identity', () {
      final cmake = File('linux/CMakeLists.txt').readAsStringSync();
      expect(cmake, contains('set(APPLICATION_ID "org.fedi3.client")'));
    });
  });
}
