/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:http/io_client.dart';

/// Single entry point for every HTTP client used by the app.
///
/// D1/H-50: requests must never hang forever. The shared client is configured
/// with an 8 s connection timeout and every request is additionally bounded by
/// [totalTimeout] (15 s). Tests replace [client] with a fake through
/// [useTestClient].
class CoreHttp {
  CoreHttp._();

  /// Maximum time allowed to establish the TCP/TLS connection.
  static const Duration connectionTimeout = Duration(seconds: 8);

  /// Maximum total time allowed for a request, including the response body.
  static const Duration totalTimeout = Duration(seconds: 15);

  static http.Client? _client;

  /// The client every [CoreApi] instance uses unless one is injected.
  static http.Client get client => _client ??= _defaultClient();

  /// Replace the shared client (tests only).
  static set client(http.Client value) => _client = value;

  static http.Client _defaultClient() {
    final inner = HttpClient()..connectionTimeout = connectionTimeout;
    return IOClient(inner);
  }

  /// Install a deterministic client for tests.
  @visibleForTesting
  static void useTestClient(http.Client testClient) {
    _client = testClient;
  }

  /// Restore the default IO client (tests only).
  @visibleForTesting
  static void reset() {
    _client?.close();
    _client = null;
  }
}
