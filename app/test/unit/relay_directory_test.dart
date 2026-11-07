/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:fedi3/model/relay_directory.dart';
import 'package:fedi3/services/relay_directory_service.dart';
import 'package:fedi3/state/relay_directory_store.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';

const _schema = 'fedi3-relay-directory/v1';

String _directory(List<Map<String, dynamic>> relays) => jsonEncode({
      'schema': _schema,
      'updated_at': '2026-11-07T12:00:00Z',
      'relays': relays,
    });

Map<String, dynamic> _relay(String url,
        {String? ws,
        String name = 'Relay',
        String operator = 'Alice',
        String contact = 'https://example.test/@alice',
        String added = '2026-11-07'}) =>
    {
      'url': url,
      if (ws != null) 'ws': ws,
      'name': name,
      'operator': operator,
      'contact': contact,
      'added': added,
    };

void main() {
  group('relay directory parsing', () {
    test('accepts valid entries and infers the websocket URL', () {
      final entries = parseRelayDirectory(_directory([
        _relay('https://relay-a.example.test'),
        _relay('https://relay-b.example.test',
            ws: 'wss://relay-b.example.test'),
      ]));
      expect(entries, hasLength(2));
      expect(entries[0].ws, 'wss://relay-a.example.test');
      expect(entries[1].ws, 'wss://relay-b.example.test');
      expect(entries[0].host, 'relay-a.example.test');
    });

    test('skips malformed entries without dropping the whole list', () {
      final entries = parseRelayDirectory(_directory([
        _relay('http://insecure.example.test'),
        _relay('https://relay.example.test', name: ''),
        _relay('https://relay.example.test/'),
        _relay('https://relay.example.test', ws: 'wss://other.example.test'),
        _relay('https://relay.example.test'),
        _relay('https://relay.example.test'),
      ]));
      expect(entries, hasLength(1));
      expect(entries.single.url, 'https://relay.example.test');
    });

    test('allows loopback http entries for local relays', () {
      final entries = parseRelayDirectory(_directory([
        _relay('http://127.0.0.1:8787', ws: 'ws://127.0.0.1:8787'),
      ]));
      expect(entries, hasLength(1));
      expect(entries.single.ws, 'ws://127.0.0.1:8787');
    });

    test('rejects an unknown schema', () {
      final raw = jsonEncode({
        'schema': 'fedi3-relay-directory/v2',
        'updated_at': '2026-11-07T12:00:00Z',
        'relays': [_relay('https://relay.example.test')],
      });
      expect(() => parseRelayDirectory(raw),
          throwsA(isA<RelayDirectoryFormatException>()));
    });

    test('rejects invalid JSON', () {
      expect(() => parseRelayDirectory('{not json'),
          throwsA(isA<RelayDirectoryFormatException>()));
    });
  });

  group('relay latency ranking', () {
    RelayLatency probe(int? ms, RelayProbeFailure? failure) => RelayLatency(
          entry: const RelayDirectoryEntry(
            url: 'https://relay.example.test',
            ws: 'wss://relay.example.test',
            name: 'Relay',
            operator: 'Alice',
            contact: 'https://example.test',
            region: null,
            added: '2026-11-07',
          ),
          latencyMs: ms,
          failure: failure,
          measuredAt: DateTime(2026, 11, 7),
        );

    test('sorts reachable relays by latency and failures last', () {
      final ranked = RelayDirectoryService.rank([
        probe(null, RelayProbeFailure.network),
        probe(80, null),
        probe(null, RelayProbeFailure.tlsHostMismatch),
        probe(20, null),
      ]);
      expect(ranked.map((r) => r.latencyMs).toList(), [20, 80, null, null]);
      expect(ranked[2].failure, RelayProbeFailure.tlsHostMismatch);
      expect(ranked[3].failure, RelayProbeFailure.network);
    });

    test('classifies TLS hostname mismatch errors', () {
      final failure = RelayDirectoryService.classifyProbeFailure(
          Exception('HandshakeException: Handshake error in client (OS Error: '
              'TLSV1_ALERT_UNRECOGNIZED_NAME(tls_record.cc:491))'));
      expect(failure, RelayProbeFailure.tlsHostMismatch);
    });
  });

  group('relay directory service', () {
    late Directory tempDir;

    setUp(() {
      tempDir = Directory.systemTemp.createTempSync('fedi3_relay_dir_test_');
      RelayDirectoryStore.useDirectoryForTesting(tempDir);
    });

    tearDown(() {
      RelayDirectoryStore.useDirectoryForTesting(null);
      tempDir.deleteSync(recursive: true);
    });

    test('fetches, parses and caches the directory', () async {
      final client = FakeHttpClient();
      client.json('GET', '/relays.json', {
        'schema': _schema,
        'updated_at': '2026-11-07T12:00:00Z',
        'relays': [_relay('https://relay.example.test')],
      });
      final snapshot = await RelayDirectoryService.fetch(
        sources: const ['https://mirror.example.test/relays.json'],
        client: client,
      );
      expect(snapshot.entries, hasLength(1));
      expect(snapshot.sourceUrl, 'https://mirror.example.test/relays.json');
      expect(await RelayDirectoryStore.cachedRaw(), isNotNull);
    });

    test('falls back to the next source when the first fails', () async {
      final client = FakeHttpClient();
      client.route('GET', '/primary.json',
          FakeHttpResponse(statusCode: 503, body: 'nope'));
      client.json('GET', '/fallback.json', {
        'schema': _schema,
        'updated_at': '2026-11-07T12:00:00Z',
        'relays': [_relay('https://relay.example.test')],
      });
      final snapshot = await RelayDirectoryService.fetch(
        sources: const [
          'https://mirror.example.test/primary.json',
          'https://mirror.example.test/fallback.json',
        ],
        client: client,
      );
      expect(snapshot.sourceUrl, 'https://mirror.example.test/fallback.json');
    });

    test('probes a reachable relay and reports latency', () async {
      final client = FakeHttpClient();
      client.route('GET', '/healthz', FakeHttpResponse(body: 'ok'));
      final entry =
          RelayDirectoryEntry.tryParse(_relay('https://relay.example.test'))!;
      final latency =
          await RelayDirectoryService.probeOne(entry, client: client);
      expect(latency.reachable, isTrue);
      expect(latency.latencyMs, isNotNull);
      expect(latency.latencyMs, greaterThanOrEqualTo(0));
    });

    test('reports HTTP failures instead of a latency value', () async {
      final client = FakeHttpClient();
      client.route(
          'GET', '/healthz', FakeHttpResponse(statusCode: 500, body: 'down'));
      final entry =
          RelayDirectoryEntry.tryParse(_relay('https://relay.example.test'))!;
      final latency =
          await RelayDirectoryService.probeOne(entry, client: client);
      expect(latency.reachable, isFalse);
      expect(latency.failure, RelayProbeFailure.httpStatus);
    });
  });
}
