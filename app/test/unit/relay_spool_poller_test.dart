/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

import 'package:fedi3/model/core_config.dart';
import 'package:fedi3/services/relay_spool_poller.dart';

CoreConfig _config({String relayWs = 'wss://relay.example.org/ws'}) =>
    CoreConfig.fromJson({
      'username': 'alice',
      'domain': 'relay.example.org',
      'publicBaseUrl': 'https://relay.example.org',
      'relayWs': relayWs,
      'relayToken': '0123456789abcdef',
      'bind': '127.0.0.1:8788',
      'internalToken': 'internal-token',
      'p2pEnable': false,
      'displayName': '',
      'summary': '',
      'iconUrl': '',
      'iconMediaType': '',
      'imageUrl': '',
      'imageMediaType': '',
      'manuallyApprovesFollowers': false,
      'blockedDomains': [],
      'blockedActors': [],
      'apRelays': [],
    });

void main() {
  late Directory dir;

  setUp(() async {
    dir = await Directory.systemTemp.createTemp('fedi3-spool-test');
  });

  tearDown(() async {
    await dir.delete(recursive: true);
  });

  test('polls the relay spool and wakes on new items', () async {
    var wakes = 0;
    var seenAfter = '';
    final client = MockClient((request) async {
      seenAfter = request.url.queryParameters['after'] ?? '';
      expect(request.url.path, '/_fedi3/chat/spool');
      expect(request.headers['Authorization'], 'Bearer 0123456789abcdef');
      return http.Response(
        jsonEncode({
          'items': [
            {
              'id': 7,
              'envelope': {'kind': 'chat'}
            },
          ],
          'latest_id': 7,
        }),
        200,
      );
    });
    final poller = RelaySpoolPoller(
      config: _config(),
      directory: dir,
      onNewItems: () async => wakes++,
      interval: const Duration(hours: 1),
      client: client,
    );
    await poller.start();
    expect(wakes, 1);
    expect(seenAfter, '0');
    final cursor = jsonDecode(
      await File('${dir.path}${Platform.pathSeparator}relay_spool.json')
          .readAsString(),
    );
    expect(cursor['last_id'], 7);
    await poller.stop();
  });

  test('empty spool does not wake the core', () async {
    var wakes = 0;
    final client = MockClient((request) async {
      return http.Response(
        jsonEncode({'items': <dynamic>[], 'latest_id': 3}),
        200,
      );
    });
    final poller = RelaySpoolPoller(
      config: _config(),
      directory: dir,
      onNewItems: () async => wakes++,
      interval: const Duration(hours: 1),
      client: client,
    );
    await poller.start();
    expect(wakes, 0);
    await poller.stop();
  });

  test('stays disabled without a relay', () async {
    var wakes = 0;
    final poller = RelaySpoolPoller(
      config: _config(relayWs: ''),
      directory: dir,
      onNewItems: () async => wakes++,
    );
    expect(poller.enabled, isFalse);
    await poller.start();
    expect(wakes, 0);
    await poller.stop();
  });
}
