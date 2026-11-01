/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

import 'package:http/http.dart' as http;

enum RelayVerifyStatus {
  ok,
  tokenTooShort,
  invalidUsername,
  missingBaseUrl,
  adminRequired,
  tokenInvalid,
  unreachable,
  badResponse,
}

class RelayVerifyResult {
  RelayVerifyResult({
    required this.status,
    this.detail,
    this.me,
  });

  final RelayVerifyStatus status;
  final String? detail;
  final Map<String, dynamic>? me;
}

String? inferRelayBaseUrl({
  required String publicBaseUrl,
  required String relayWs,
}) {
  final base = publicBaseUrl.trim();
  if (base.isNotEmpty) {
    return base.replaceAll(RegExp(r'/$'), '');
  }
  final ws = relayWs.trim();
  if (ws.startsWith('wss://')) {
    return 'https://${ws.substring('wss://'.length)}'.replaceAll(RegExp(r'/$'), '');
  }
  if (ws.startsWith('ws://')) {
    return 'http://${ws.substring('ws://'.length)}'.replaceAll(RegExp(r'/$'), '');
  }
  if (ws.startsWith('https://') || ws.startsWith('http://')) {
    return ws.replaceAll(RegExp(r'/$'), '');
  }
  return null;
}

Future<RelayVerifyResult> verifyRelay({
  required String username,
  required String relayToken,
  required String publicBaseUrl,
  required String relayWs,
}) async {
  final user = username.trim();
  if (user.isEmpty) {
    return RelayVerifyResult(status: RelayVerifyStatus.invalidUsername);
  }
  final token = relayToken.trim();
  if (token.length < 16) {
    return RelayVerifyResult(status: RelayVerifyStatus.tokenTooShort);
  }

  final base = inferRelayBaseUrl(publicBaseUrl: publicBaseUrl, relayWs: relayWs);
  if (base == null || base.isEmpty) {
    return RelayVerifyResult(status: RelayVerifyStatus.missingBaseUrl);
  }

  final baseUri = Uri.tryParse(base);
  if (baseUri == null || baseUri.host.isEmpty) {
    return RelayVerifyResult(status: RelayVerifyStatus.missingBaseUrl);
  }

  final client = http.Client();
  try {
    final meUri = baseUri.replace(path: '/_fedi3/relay/me', queryParameters: {'username': user});
    final meResp = await client.get(
      meUri,
      headers: {'Authorization': 'Bearer $token'},
    );
    if (meResp.statusCode >= 200 && meResp.statusCode < 300) {
      final json = jsonDecode(meResp.body);
      if (json is Map<String, dynamic>) {
        final tokenOk = json['token_ok'];
        final known = json['known'];
        if (tokenOk == true) {
          return RelayVerifyResult(status: RelayVerifyStatus.ok, me: json);
        }
        if (known == true && tokenOk == false) {
          return RelayVerifyResult(status: RelayVerifyStatus.tokenInvalid, me: json);
        }
      }
    }

    final regUri = baseUri.replace(path: '/register');
    final regResp = await client.post(
      regUri,
      headers: const {'Content-Type': 'application/json'},
      body: jsonEncode({'username': user, 'token': token}),
    );
    if (regResp.statusCode == 401 && regResp.body.contains('admin token required')) {
      return RelayVerifyResult(status: RelayVerifyStatus.adminRequired);
    }
    if (regResp.statusCode == 401 && regResp.body.contains('invalid token')) {
      return RelayVerifyResult(status: RelayVerifyStatus.tokenInvalid);
    }
    if (regResp.statusCode == 400 && regResp.body.contains('token too short')) {
      return RelayVerifyResult(status: RelayVerifyStatus.tokenTooShort);
    }
    if (regResp.statusCode < 200 || regResp.statusCode >= 300) {
      return RelayVerifyResult(
        status: RelayVerifyStatus.badResponse,
        detail: '${regResp.statusCode} ${regResp.body}'.trim(),
      );
    }

    final meAfter = await client.get(
      meUri,
      headers: {'Authorization': 'Bearer $token'},
    );
    if (meAfter.statusCode == 401) {
      return RelayVerifyResult(status: RelayVerifyStatus.tokenInvalid);
    }
    if (meAfter.statusCode < 200 || meAfter.statusCode >= 300) {
      return RelayVerifyResult(
        status: RelayVerifyStatus.badResponse,
        detail: '${meAfter.statusCode} ${meAfter.body}'.trim(),
      );
    }
    final json = jsonDecode(meAfter.body);
    if (json is Map<String, dynamic>) {
      if (json['token_ok'] == false) {
        return RelayVerifyResult(status: RelayVerifyStatus.tokenInvalid, me: json);
      }
      return RelayVerifyResult(status: RelayVerifyStatus.ok, me: json);
    }
    return RelayVerifyResult(status: RelayVerifyStatus.badResponse, detail: 'invalid json');
  } catch (e) {
    return RelayVerifyResult(
      status: RelayVerifyStatus.unreachable,
      detail: e.toString(),
    );
  } finally {
    client.close();
  }
}
