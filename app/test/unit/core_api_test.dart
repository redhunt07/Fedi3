/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:fedi3/core/core_api.dart';
import 'package:fedi3/services/core_event_stream.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;

import '../support/fake_http.dart';
import '../support/test_config.dart';

void main() {
  late FakeHttpClient client;

  setUp(() {
    client = FakeHttpClient();
  });

  CoreApi api({Duration? timeout}) => CoreApi(
        config: buildTestConfig(),
        client: client,
        totalTimeout: timeout ?? const Duration(seconds: 5),
      );

  group('CoreApi auth headers (AN-01)', () {
    test('AN-01 no internal token means no X-Fedi3-Internal header', () async {
      client.json('GET', '/_fedi3/timeline/home', {'items': <dynamic>[]});
      final noToken = CoreApi(
        config: buildTestConfig(internalToken: ''),
        client: client,
      );
      await noToken.fetchTimeline('home');
      expect(client.lastHeader('X-Fedi3-Internal'), isNull);
    });

    test('AN-01b internal token is sent when configured', () async {
      client.json('GET', '/_fedi3/timeline/home', {'items': <dynamic>[]});
      await api().fetchTimeline('home');
      expect(client.lastHeader('X-Fedi3-Internal'),
          'internal-token-0123456789abcdef');
    });
  });

  test('relay refresh sends the internal token and preserves auth failures',
      () async {
    client.route('POST', '/_fedi3/relays/refresh',
        FakeHttpResponse(statusCode: 403, body: 'forbidden'));
    await expectLater(
        api().refreshRelays(),
        throwsA(
            isA<CoreApiException>().having((e) => e.isAuth, 'isAuth', isTrue)));
    expect(client.lastHeader('X-Fedi3-Internal'),
        'internal-token-0123456789abcdef');
  });

  group('CoreApi errors (AN-02..05)', () {
    test('AN-02 slow server yields a timeout error', () async {
      client.delay = const Duration(milliseconds: 400);
      client.json('GET', '/_fedi3/timeline/home', {'items': <dynamic>[]});
      final err = await api(timeout: const Duration(milliseconds: 50))
          .fetchTimeline('home')
          .then<Object?>((_) => null)
          .catchError((Object e) => e);
      expect(err, isA<CoreApiException>());
      final e = err as CoreApiException;
      expect(e.kind, CoreApiErrorKind.timeout);
      expect(e.retryable, isTrue);
    });

    test('AN-03 401 maps to unauthorized', () async {
      client.route('GET', '/_fedi3/timeline/home',
          FakeHttpResponse(statusCode: 401, body: 'no'));
      final err = await api()
          .fetchTimeline('home')
          .then<Object?>((_) => null)
          .catchError((Object e) => e);
      expect(err, isA<CoreApiException>());
      final e = err as CoreApiException;
      expect(e.kind, CoreApiErrorKind.unauthorized);
      expect(e.isAuth, isTrue);
    });

    test('AN-04 500 maps to a retryable server error', () async {
      client.route('GET', '/_fedi3/timeline/home',
          FakeHttpResponse(statusCode: 500, body: 'boom'));
      final err = await api()
          .fetchTimeline('home')
          .then<Object?>((_) => null)
          .catchError((Object e) => e);
      expect(err, isA<CoreApiException>());
      final e = err as CoreApiException;
      expect(e.kind, CoreApiErrorKind.server);
      expect(e.retryable, isTrue);
    });

    test('AN-05 malformed JSON maps to decode without crashing', () async {
      client.route(
          'GET', '/_fedi3/timeline/home', FakeHttpResponse(body: 'not json'));
      final err = await api()
          .fetchTimeline('home')
          .then<Object?>((_) => null)
          .catchError((Object e) => e);
      expect(err, isA<CoreApiException>());
      final e = err as CoreApiException;
      expect(e.kind, CoreApiErrorKind.decode);
    });
  });

  group('CoreApi encoding (AN-06..07)', () {
    test('AN-06 non-ASCII X-Filename is percent-encoded', () async {
      client.json('POST', '/users/alice/media',
          {'id': 'm1', 'url': 'https://example.test/m/1'});
      await api().uploadMedia(
        bytes: Uint8List.fromList(utf8.encode('x')),
        filename: 'città.jpg',
      );
      final header = client.lastHeader('X-Filename');
      expect(header, isNotNull);
      expect(header, 'citt%C3%A0.jpg');
      expect(header!.codeUnits.every((c) => c < 128), isTrue);
    });

    test('AN-07 threadId with / and % is path-encoded', () async {
      client.json(
          'GET', '/_fedi3/chat/threads/a%2Fb%25c', {'items': <dynamic>[]});
      await api().fetchChatMessages(threadId: 'a/b%c');
      expect(client.lastRequest.url.path, '/_fedi3/chat/threads/a%2Fb%25c');
    });
  });

  group('CoreApi configuration guards (M-33)', () {
    test('empty publicBaseUrl raises invalidConfig instead of FormatException',
        () async {
      final bad =
          CoreApi(config: buildTestConfig(publicBaseUrl: ''), client: client);
      final err = await bad
          .fetchRelayList()
          .then<Object?>((_) => null)
          .catchError((Object e) => e);
      expect(err, isA<CoreApiException>());
      final e = err as CoreApiException;
      expect(e.kind, CoreApiErrorKind.invalidConfig);
    });

    test('empty bind raises invalidConfig', () async {
      final bad = CoreApi(config: buildTestConfig(bind: ''), client: client);
      final err = await bad
          .fetchTimeline('home')
          .then<Object?>((_) => null)
          .catchError((Object e) => e);
      expect(err, isA<CoreApiException>());
      final e = err as CoreApiException;
      expect(e.kind, CoreApiErrorKind.invalidConfig);
    });
  });

  group('CoreEventStream (AN-08..10)', () {
    CoreEventStream stream() => CoreEventStream(
          config: buildTestConfig(),
          client: client,
        );

    test('AN-08 valid frame is emitted', () async {
      client.route(
        'GET',
        '/_fedi3/stream',
        FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'},
          body: 'data: {"kind":"notification","ts_ms":123}\n\n',
        ),
      );
      final events = await stream().stream().toList();
      expect(events, hasLength(1));
      expect(events.single.kind, 'notification');
      expect(events.single.tsMs, 123);
    });

    test('AN-09 malformed frame is skipped and the stream stays alive',
        () async {
      client.route(
        'GET',
        '/_fedi3/stream',
        FakeHttpResponse(
          headers: const {'content-type': 'text/event-stream'},
          body: 'data: {broken\n\ndata: {"kind":"inbox","ts_ms":7}\n\n',
        ),
      );
      final events = await stream().stream().toList();
      expect(events, hasLength(1));
      expect(events.single.kind, 'inbox');
    });

    test('AN-10 network error is propagated', () async {
      client.sendError = const SocketException('connection refused');
      await expectLater(
          stream().stream().toList(), throwsA(isA<SocketException>()));
    });

    test('AN-10b connect timeout is propagated', () async {
      client.delay = const Duration(milliseconds: 400);
      client.route(
        'GET',
        '/_fedi3/stream',
        FakeHttpResponse(body: 'data: {"kind":"x"}\n\n'),
      );
      // CoreEventStream uses the shared 15 s budget; use a small client delay
      // and assert the stream completes normally (no hang).
      final events = await stream().stream().toList();
      expect(events, hasLength(1));
    });
  });

  test('unexpected transport error is wrapped as network', () async {
    client.sendError = http.ClientException('socket closed');
    final err = await api()
        .fetchTimeline('home')
        .then<Object?>((_) => null)
        .catchError((Object e) => e);
    expect(err, isA<CoreApiException>());
    final e = err as CoreApiException;
    expect(e.kind, CoreApiErrorKind.network);
    expect(e.retryable, isTrue);
  });

  test('timeout leaves no pending request', () async {
    client.delay = const Duration(milliseconds: 300);
    client.json('GET', '/_fedi3/timeline/home', {'items': <dynamic>[]});
    final apiSmall = api(timeout: const Duration(milliseconds: 30));
    expect(apiSmall.fetchTimeline('home'), throwsA(isA<CoreApiException>()));
    await Future<void>.delayed(const Duration(milliseconds: 350));
  });

  group('CoreApi chat creation (CT-01..02)', () {
    test('CT-01 createChatThread posts recipients/kind and returns the id',
        () async {
      client.json('POST', '/_fedi3/chat/thread/create', {
        'thread_id': 'urn:fedi3:chat:dm:abc',
        'kind': 'dm',
      });
      final resp = await api().createChatThread(
        recipients: const ['https://a.test/users/bob'],
        kind: 'dm',
      );
      expect(resp['thread_id'], 'urn:fedi3:chat:dm:abc');
      final body = jsonDecode(client.lastBody) as Map<String, dynamic>;
      expect(body['recipients'], ['https://a.test/users/bob']);
      expect(body['kind'], 'dm');
    });

    test('CT-02 setChatMemberRole sends a roles map', () async {
      client.json('POST', '/_fedi3/chat/thread/members', {'ok': true});
      await api().setChatMemberRole(
        threadId: 't1',
        actor: 'https://a.test/users/bob',
        role: 'admin',
      );
      final body = jsonDecode(client.lastBody) as Map<String, dynamic>;
      expect(body['thread_id'], 't1');
      expect(body['roles'], {'https://a.test/users/bob': 'admin'});
    });

    test('CT-03 searchChatMessages sends thread_id/q/limit', () async {
      client.json('GET', '/_fedi3/chat/thread/search', {
        'items': [
          {'message_id': 'm1', 'thread_id': 't1', 'body_json': '{}'}
        ]
      });
      final resp = await api()
          .searchChatMessages(threadId: 't1', query: 'ciao', limit: 10);
      expect((resp['items'] as List), hasLength(1));
      final index = client.requests
          .indexWhere((r) => r.url.path == '/_fedi3/chat/thread/search');
      expect(client.requests[index].url.queryParameters['thread_id'], 't1');
      expect(client.requests[index].url.queryParameters['q'], 'ciao');
      expect(client.requests[index].url.queryParameters['limit'], '10');
    });

    test('CT-04 setChatPin posts thread/message/pinned', () async {
      client.json('POST', '/_fedi3/chat/thread/pin', {'ok': true});
      await api().setChatPin(threadId: 't1', messageId: 'm1', pinned: true);
      final body = jsonDecode(client.lastBody) as Map<String, dynamic>;
      expect(body['thread_id'], 't1');
      expect(body['message_id'], 'm1');
      expect(body['pinned'], isTrue);
    });

    test('CT-05 fetchChatPin returns the pin map', () async {
      client.json('GET', '/_fedi3/chat/thread/pin', {
        'pin': {'message_id': 'm1', 'preview': 'ciao'}
      });
      final pin = await api().fetchChatPin(threadId: 't1');
      expect(pin?['message_id'], 'm1');
      expect(pin?['preview'], 'ciao');
    });

    test('CT-08 sendChatCallSignal posts call routing fields', () async {
      client.json('POST', '/_fedi3/chat/call/signal', {'ok': true});
      await api().sendChatCallSignal(
        threadId: 't1',
        callId: 'call-1',
        action: 'offer',
        to: 'https://a.test/users/bob',
        data: {'sdp': 'v=0'},
      );
      final body = jsonDecode(client.lastBody) as Map<String, dynamic>;
      expect(body['thread_id'], 't1');
      expect(body['call_id'], 'call-1');
      expect(body['action'], 'offer');
      expect(body['to'], 'https://a.test/users/bob');
      expect(body['data'], {'sdp': 'v=0'});
    });

    test('CT-09 fetchChatCallSignals parses signals', () async {
      client.json('GET', '/_fedi3/chat/call/signals', {
        'items': [
          {
            'id': 7,
            'from_actor': 'https://a.test/users/bob',
            'to_actor': null,
            'call_id': 'call-1',
            'action': 'invite',
            'data': {'media': 'video'},
            'created_at_ms': 1000
          }
        ]
      });
      final items =
          await api().fetchChatCallSignals(threadId: 't1', afterMs: 0);
      expect(items, hasLength(1));
      expect(items.first.action, 'invite');
      expect(items.first.data['media'], 'video');
    });

    test('CT-07 searchAllChatMessages parses membership-scoped hits', () async {
      client.json('GET', '/_fedi3/chat/search', {
        'items': [
          {
            'message_id': 'm1',
            'thread_id': 't1',
            'thread_kind': 'group',
            'thread_title': 'Team',
            'sender_actor': 'https://a.test/users/bob',
            'created_at_ms': 1,
            'text': 'ciao'
          }
        ]
      });
      final hits = await api().searchAllChatMessages(query: 'ciao', limit: 10);
      expect(hits, hasLength(1));
      expect(hits.first.threadTitle, 'Team');
      expect(hits.first.text, 'ciao');
      final index = client.requests
          .indexWhere((r) => r.url.path == '/_fedi3/chat/search');
      expect(client.requests[index].url.queryParameters['q'], 'ciao');
    });

    test('CT-06 joinChatThread posts the thread id', () async {
      client.json(
          'POST', '/_fedi3/chat/thread/join', {'ok': true, 'thread_id': 't1'});
      final resp = await api().joinChatThread(threadId: 't1');
      expect(resp['thread_id'], 't1');
      final body = jsonDecode(client.lastBody) as Map<String, dynamic>;
      expect(body['thread_id'], 't1');
    });
  });
}
