/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/fedi3_core.dart';

/// Test double for the native FFI core (AP-01..06).
class FakeCoreRuntime implements CoreRuntime {
  FakeCoreRuntime({this.handle = 1, this.startError, this.stopError});

  int handle;
  Object? startError;
  Object? stopError;

  int startCalls = 0;
  int stopCalls = 0;
  String? lastConfigJson;
  final List<int> stoppedHandles = <int>[];

  @override
  int startJson(String configJson) {
    startCalls++;
    lastConfigJson = configJson;
    final error = startError;
    if (error != null) throw error;
    return handle;
  }

  @override
  void stop(int handle) {
    stopCalls++;
    stoppedHandles.add(handle);
    final error = stopError;
    if (error != null) throw error;
  }
}
