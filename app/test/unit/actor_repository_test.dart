import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/services/actor_repository.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';
import '../support/test_config.dart';

void main() {
  late FakeHttpClient client;
  setUp(() {
    client = FakeHttpClient();
    CoreHttp.useTestClient(client);
  });
  tearDown(CoreHttp.reset);

  test('followers accept an embedded collection page', () async {
    client.json('GET', '/followers', {
      'first': {
        'type': 'CollectionPage',
        'items': ['https://remote.test/users/a']
      },
    });
    expect(
        await ActorRepository.instance
            .fetchCollectionItems('https://remote.test/followers'),
        ['https://remote.test/users/a']);
  });

  test('own collections use the local core and remote collections stay remote',
      () async {
    final cfg = buildTestConfig();
    final own = '${cfg.publicBaseUrl}/users/${cfg.username}/followers';
    client.json('GET', '/users/${cfg.username}/followers', {'totalItems': 2});
    expect(
        await ActorRepository.instance.fetchCollectionCount(own, config: cfg),
        2);
    expect(client.lastRequest.url.host, cfg.localBaseUri.host);
    expect(client.lastRequest.url.port, cfg.localBaseUri.port);
    await ActorRepository.instance.fetchCollectionCount(
        'https://other.test/users/alice/followers',
        config: cfg);
    expect(client.lastRequest.url.host, 'other.test');
  });

  test('outbox accepts embedded pages and bare notes without dropping items',
      () async {
    client.json('GET', '/outbox', {
      'first': {
        'id': 'https://remote.test/page',
        'type': 'CollectionPage',
        'items': List.generate(
            25,
            (i) => {
                  'id': 'https://remote.test/notes/$i',
                  'type': 'Note',
                  'attributedTo': 'https://remote.test/users/a',
                  'attachment': [
                    {'type': 'Image', 'url': 'https://remote.test/photo.jpg'}
                  ],
                }),
        'next': 'https://remote.test/next',
      },
    });
    final page = await ActorRepository.instance
        .fetchOutboxPage('https://remote.test/outbox', limit: 20);
    expect(page.items, hasLength(25));
    expect(page.items.first['type'], 'Create');
    expect(page.items.first['actor'], 'https://remote.test/users/a');
    expect((page.items.first['object'] as Map)['attachment'], isNotEmpty);
    expect(page.next, 'https://remote.test/next');
    expect(client.requests, hasLength(1));
  });

  test('own outbox pages use the local core', () async {
    final cfg = buildTestConfig();
    final own = '${cfg.publicBaseUrl}/users/${cfg.username}/outbox';
    client.json('GET', '/users/${cfg.username}/outbox', {
      'orderedItems': [
        {'id': 'activity', 'type': 'Create'}
      ],
    });
    final page =
        await ActorRepository.instance.fetchOutboxPage(own, config: cfg);
    expect(page.items, hasLength(1));
    expect(client.lastRequest.url.host, cfg.localBaseUri.host);
    expect(client.lastRequest.url.port, cfg.localBaseUri.port);
  });

  test('malformed actor response does not break notification rows', () async {
    client.route('GET', '/users/broken',
        FakeHttpResponse(body: '<html>unavailable</html>'));
    expect(
        await ActorRepository.instance
            .getActor('https://broken.test/users/broken'),
        isNull);
  });
}
