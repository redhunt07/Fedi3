/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'package:fedi3/core/core_http.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;

import '../support/fake_http.dart';

void main() {
  tearDown(CoreHttp.reset);

  test('dropConnections replaces the pooled keep-alive client', () {
    CoreHttp.reset();
    final first = CoreHttp.client;
    CoreHttp.dropConnections();
    final second = CoreHttp.client;
    expect(identical(first, second), isFalse,
        reason: 'a dead pooled socket must not be reused after a core restart');
  });

  test('dropConnections keeps an injected test client', () {
    final injected = FakeHttpClient();
    CoreHttp.useTestClient(injected);
    CoreHttp.dropConnections();
    expect(identical(CoreHttp.client, injected), isTrue);
  });

  test('dropConnections is a no-op when no client was created', () {
    CoreHttp.reset();
    expect(() => CoreHttp.dropConnections(), returnsNormally);
    expect(CoreHttp.client, isA<http.Client>());
  });
}
