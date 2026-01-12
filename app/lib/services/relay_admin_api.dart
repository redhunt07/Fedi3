/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:http/http.dart' as http;

class RelayAdminApi {
  RelayAdminApi({required this.relayWs, required this.adminToken});

  final String relayWs;
  final String adminToken;

  Uri _baseUri() {
    final ws = relayWs.trim();
    if (ws.startsWith('wss://')) {
      return Uri.parse('https://${ws.substring('wss://'.length)}');
    }
    if (ws.startsWith('ws://')) {
      return Uri.parse('http://${ws.substring('ws://'.length)}');
    }
    return Uri.parse(ws);
  }

  Map<String, String> get _headers => {
        'Authorization': 'Bearer ${adminToken.trim()}',
      };

  Future<List<Map<String, dynamic>>> listUsers({int limit = 200, int offset = 0}) async {
    final base = _baseUri();
    final uri = base.replace(path: '/admin/users', queryParameters: {
      'limit': limit.toString(),
      'offset': offset.toString(),
    });
    final resp = await http.get(uri, headers: _headers);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin list users failed: ${resp.statusCode} ${resp.body}');
    }
    final data = jsonDecode(resp.body);
    if (data is List) {
      return data.map((e) => Map<String, dynamic>.from(e as Map)).toList();
    }
    return const [];
  }

  Future<Map<String, dynamic>> getUser(String username) async {
    final base = _baseUri();
    final uri = base.replace(path: '/admin/users/$username');
    final resp = await http.get(uri, headers: _headers);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin get user failed: ${resp.statusCode} ${resp.body}');
    }
    return Map<String, dynamic>.from(jsonDecode(resp.body) as Map);
  }

  Future<void> disableUser(String username) async {
    final base = _baseUri();
    final uri = base.replace(path: '/admin/users/$username/disable');
    final resp = await http.post(uri, headers: _headers);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin disable user failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<void> enableUser(String username) async {
    final base = _baseUri();
    final uri = base.replace(path: '/admin/users/$username/enable');
    final resp = await http.post(uri, headers: _headers);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin enable user failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<String> rotateToken(String username) async {
    final base = _baseUri();
    final uri = base.replace(path: '/admin/users/$username/rotate_token');
    final resp = await http.post(uri, headers: _headers);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin rotate token failed: ${resp.statusCode} ${resp.body}');
    }
    final data = jsonDecode(resp.body) as Map;
    return data['token']?.toString() ?? '';
  }

  Future<void> deleteUser(String username) async {
    final base = _baseUri();
    final uri = base.replace(path: '/admin/users/$username');
    final resp = await http.delete(uri, headers: _headers);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin delete user failed: ${resp.statusCode} ${resp.body}');
    }
  }

  Future<String> registerUser(String username, String token) async {
    final base = _baseUri();
    final uri = base.replace(path: '/register');
    final resp = await http.post(
      uri,
      headers: {
        ..._headers,
        'Content-Type': 'application/json',
      },
      body: jsonEncode({'username': username, 'token': token}),
    );
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin register failed: ${resp.statusCode} ${resp.body}');
    }
    return resp.body.toString();
  }

  Future<List<Map<String, dynamic>>> listAudit({int limit = 200, int offset = 0}) async {
    final base = _baseUri();
    final uri = base.replace(path: '/admin/audit', queryParameters: {
      'limit': limit.toString(),
      'offset': offset.toString(),
    });
    final resp = await http.get(uri, headers: _headers);
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('admin audit failed: ${resp.statusCode} ${resp.body}');
    }
    final data = jsonDecode(resp.body);
    if (data is List) {
      return data.map((e) => Map<String, dynamic>.from(e as Map)).toList();
    }
    return const [];
  }
}
