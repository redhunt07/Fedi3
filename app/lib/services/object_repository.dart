import '../core/core_http.dart';
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:http/http.dart' as http;

class ObjectRepository {
  ObjectRepository._();

  static final ObjectRepository instance = ObjectRepository._();

  http.Client get _client => CoreHttp.client;

  Map<String, String> get _acceptHeaders => const {
        'Accept':
            'application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams", application/json',
      };

  Future<Map<String, dynamic>?> fetchObject(String url) async {
    final u = url.trim();
    if (u.isEmpty) return null;
    final uri = Uri.tryParse(u);
    if (uri == null || uri.host.isEmpty) return null;
    final resp = await _client
        .get(uri, headers: _acceptHeaders)
        .timeout(CoreHttp.totalTimeout);
    if (resp.statusCode < 200 || resp.statusCode >= 300) return null;
    final json = jsonDecode(resp.body);
    if (json is! Map) return null;
    return json.cast<String, dynamic>();
  }
}
