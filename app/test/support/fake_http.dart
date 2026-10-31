/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

/// Deterministic HTTP response for [FakeHttpClient].
class FakeHttpResponse {
  FakeHttpResponse({
    this.statusCode = 200,
    this.body = '{}',
    Map<String, String>? headers,
  }) : headers = headers ?? const {'content-type': 'application/json'};

  final int statusCode;
  final String body;
  final Map<String, String> headers;
}

/// Test double for `package:http` (docs/test_plan.md §2).
///
/// It records every request and supports programmable routes, delays and
/// transport errors, so no test ever touches the network.
class FakeHttpClient extends http.BaseClient {
  FakeHttpClient({this.defaultResponse, this.sendError});

  /// Optional catch-all response. When null, unmatched routes return 404.
  final FakeHttpResponse? defaultResponse;

  /// Responses keyed by `METHOD /path` (query string ignored).
  final Map<String, FakeHttpResponse> routes = <String, FakeHttpResponse>{};

  /// Optional dynamic handler, receives the request and wins over [routes].
  FutureOr<FakeHttpResponse> Function(http.BaseRequest request)? handler;

  final List<http.BaseRequest> requests = <http.BaseRequest>[];
  final List<String> requestBodies = <String>[];

  /// Artificial latency applied before answering.
  Duration? delay;

  /// When set, [send] throws it (simulates a transport failure).
  Object? sendError;

  int sendCount = 0;

  void route(String method, String path, FakeHttpResponse response) {
    routes['${method.toUpperCase()} $path'] = response;
  }

  void json(String method, String path, Object body, {int statusCode = 200}) {
    route(
      method,
      path,
      FakeHttpResponse(statusCode: statusCode, body: jsonEncode(body)),
    );
  }

  http.BaseRequest get lastRequest => requests.last;

  String get lastBody => requestBodies.isEmpty ? '' : requestBodies.last;

  String? lastHeader(String name) => requests.last.headers[name];

  List<http.BaseRequest> requestsFor(String method, String path) {
    final key = '${method.toUpperCase()} $path';
    return requests
        .where((r) => '${r.method.toUpperCase()} ${r.url.path}' == key)
        .toList(growable: false);
  }

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    sendCount++;
    requests.add(request);
    if (request is http.Request) {
      requestBodies.add(request.body);
    } else {
      requestBodies.add('');
    }
    if (sendError != null) throw sendError!;
    if (delay != null) {
      await Future<void>.delayed(delay!);
    }

    final dynamicHandler = handler;
    FakeHttpResponse? response;
    if (dynamicHandler != null) {
      response = await dynamicHandler(request);
    }
    response ??= routes['${request.method.toUpperCase()} ${request.url.path}'];
    response ??= defaultResponse;
    response ??=
        FakeHttpResponse(statusCode: 404, body: '{"error":"not found"}');

    final stream = Stream<List<int>>.value(utf8.encode(response.body));
    return http.StreamedResponse(
      stream,
      response.statusCode,
      headers: response.headers,
      request: request,
    );
  }
}
