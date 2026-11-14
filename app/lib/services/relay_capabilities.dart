import 'dart:convert';

import 'package:http/http.dart' as http;

import '../core/core_http.dart';
import '../model/core_config.dart';

/// Authenticated provisioning on the selected relay, never on directory URLs.
class RelayCapabilities {
  RelayCapabilities._();

  static Uri _endpoint(CoreConfig config, String capability) {
    final relay = Uri.parse(config.relayWs);
    if (relay.scheme != 'wss' ||
        relay.host.isEmpty ||
        relay.userInfo.isNotEmpty) {
      throw StateError('Relay provisioning requires WSS/HTTPS');
    }
    return relay.replace(
      scheme: 'https',
      path: '/_fedi3/relay/$capability/${Uri.encodeComponent(config.username)}',
      query: '',
      fragment: '',
    );
  }

  static Future<http.Response> _request(
    CoreConfig config,
    String capability,
    String method, {
    Object? body,
    String? installationId,
  }) async {
    final request = http.Request(method, _endpoint(config, capability))
      ..followRedirects = false
      ..headers['Authorization'] = 'Bearer ${config.relayToken}';
    if (installationId != null) {
      request.headers['X-Fedi3-Push-Installation'] = installationId;
    }
    if (body != null) {
      request.headers['Content-Type'] = 'application/json';
      request.body = jsonEncode(body);
    }
    return _sendBounded(request);
  }

  static Future<http.Response> _sendBounded(http.Request request,
      {int limit = 65536}) async {
    // Streaming limit also applies when Content-Length is absent or dishonest.
    return (() async {
      final response = await CoreHttp.client.send(request);
      final bytes = <int>[];
      await for (final chunk in response.stream) {
        if (bytes.length + chunk.length > limit) {
          throw StateError('Relay provisioning response too large');
        }
        bytes.addAll(chunk);
      }
      return http.Response.bytes(bytes, response.statusCode);
    })()
        .timeout(const Duration(seconds: 4));
  }

  /// Platform connectors call this after obtaining an endpoint/device token.
  /// Delivery remains disabled until the relay operator configures a gateway.
  static Future<void> registerPush(
      CoreConfig config, String provider, String token,
      {Map<String, String>? webpushKeys, String? installationId}) async {
    if (!['unifiedpush', 'apns'].contains(provider)) {
      throw ArgumentError.value(provider, 'provider');
    }
    final response = await _request(config, 'push', 'PUT', body: {
      if (installationId != null) 'installation_id': installationId,
      'provider': provider,
      'token': token,
      if (webpushKeys != null) 'webpush_keys': webpushKeys,
    });
    if (response.statusCode != 200) {
      throw StateError('Push registration failed (${response.statusCode})');
    }
  }

  static Future<Map<String, dynamic>> pushConfig(CoreConfig config) async {
    final request = http.Request('GET',
        _endpoint(config, 'push').replace(path: '/_fedi3/relay/push-config'))
      ..followRedirects = false;
    final body = await _sendBounded(request, limit: 8192);
    if (body.statusCode != 200) {
      throw StateError('Push configuration unavailable');
    }
    return jsonDecode(body.body) as Map<String, dynamic>;
  }

  static Future<int> pendingPushItems(CoreConfig config) async {
    final response = await _request(config, 'push', 'GET');
    if (response.statusCode != 200) throw StateError('Push queue unavailable');
    return (jsonDecode(response.body) as Map<String, dynamic>)['pending_items']
        as int;
  }

  static Future<void> unregisterPush(CoreConfig config,
      {String? installationId}) async {
    final response = await _request(config, 'push', 'DELETE',
        installationId: installationId);
    if (response.statusCode != 204) {
      throw StateError('Push removal failed (${response.statusCode})');
    }
  }

  static Future<List<Map<String, dynamic>>> turnPool(CoreConfig config) async {
    try {
      final response = await _request(config, 'turn', 'GET');
      if (response.statusCode != 200) return [];
      return parseTurnPool(jsonDecode(response.body));
    } catch (_) {
      // ICE still tries explicit operator configuration and the STUN defaults.
      return [];
    }
  }

  static List<Map<String, dynamic>> parseTurnPool(Object? raw) {
    if (raw is! List || raw.length > 8) return [];
    final result = <Map<String, dynamic>>[];
    for (final item in raw) {
      if (item is! Map || item['urls'] is! List) return [];
      final urls = item['urls'] as List;
      final username = item['username'];
      final credential = item['credential'];
      if (urls.isEmpty ||
          urls.length > 4 ||
          username is! String ||
          username.isEmpty ||
          username.length > 256 ||
          credential is! String ||
          credential.isEmpty ||
          credential.length > 1024) {
        return [];
      }
      for (final url in urls) {
        if (url is! String ||
            url.length > 512 ||
            !(url.startsWith('turn:') || url.startsWith('turns:')) ||
            url.contains('@') ||
            RegExp(r'\s').hasMatch(url)) {
          return [];
        }
        // One URL per entry: Linux's native bridge does not retain URL lists.
        result
            .add({'urls': url, 'username': username, 'credential': credential});
      }
    }
    return result;
  }
}
