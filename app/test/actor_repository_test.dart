/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:fedi3/services/actor_repository.dart';

void main() {
  group('ActorRepository collection links', () {
    late HttpServer server;
    late String baseUrl;

    setUp(() async {
      server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      baseUrl = 'http://${server.address.address}:${server.port}';
    });

    tearDown(() async {
      await server.close(force: true);
    });

    test('resolves relative first and next links for outbox pages', () async {
      server.listen((HttpRequest req) {
        req.response.headers.contentType = ContentType.json;
        if (req.uri.path == '/users/alice/outbox' &&
            !req.uri.queryParameters.containsKey('page')) {
          req.response.write(jsonEncode({
            'type': 'OrderedCollection',
            'first': '/users/alice/outbox?page=1',
          }));
        } else if (req.uri.path == '/users/alice/outbox' &&
            req.uri.queryParameters['page'] == '1') {
          req.response.write(jsonEncode({
            'id': '/users/alice/outbox?page=1',
            'type': 'OrderedCollectionPage',
            'orderedItems': [
              {
                'type': 'Create',
                'id': '$baseUrl/activities/1',
                'actor': '$baseUrl/users/alice',
                'object': {
                  'type': 'Note',
                  'id': '$baseUrl/notes/1',
                  'content': 'hello',
                },
              }
            ],
            'next': '/users/alice/outbox?page=2',
          }));
        } else {
          req.response.statusCode = HttpStatus.notFound;
          req.response.write('{}');
        }
        req.response.close();
      });

      final repo = ActorRepository.instance;
      final page =
          await repo.fetchOutboxPage('$baseUrl/users/alice/outbox', limit: 20);

      expect(page.items, hasLength(1));
      expect(page.items.first['id'], '$baseUrl/activities/1');
      expect(page.next, '$baseUrl/users/alice/outbox?page=2');
    });

    test('uses root page orderedItems when first is absent', () async {
      server.listen((HttpRequest req) {
        req.response.headers.contentType = ContentType.json;
        switch (req.uri.path) {
          case '/users/bob/outbox':
            req.response.write(jsonEncode({
              'id': '$baseUrl/users/bob/outbox',
              'type': 'OrderedCollectionPage',
              'orderedItems': [
                {
                  'type': 'Create',
                  'id': '$baseUrl/activities/2',
                  'actor': '$baseUrl/users/bob',
                  'object': {
                    'type': 'Note',
                    'id': '$baseUrl/notes/2',
                    'content': 'root page item',
                  },
                }
              ],
            }));
            break;
          default:
            req.response.statusCode = HttpStatus.notFound;
            req.response.write('{}');
            break;
        }
        req.response.close();
      });

      final repo = ActorRepository.instance;
      final page =
          await repo.fetchOutboxPage('$baseUrl/users/bob/outbox', limit: 20);

      expect(page.items, hasLength(1));
      expect(page.items.first['id'], '$baseUrl/activities/2');
      expect(page.next, isNull);
    });
  });
}
