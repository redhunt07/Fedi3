/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:typed_data';

import 'package:fedi3/core/core_api.dart';
import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/widgets/reply_composer.dart';
import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

class _FilePicker extends FileSelectorPlatform {
  _FilePicker(this.files);
  final List<XFile> files;

  @override
  Future<List<XFile>> openFiles({
    List<XTypeGroup>? acceptedTypeGroups,
    String? initialDirectory,
    String? confirmButtonText,
  }) async =>
      files;
}

void main() {
  testWidgets('reply uploads media and preserves its parent and draft on retry',
      (tester) async {
    await setUpPumpEnv(tester);
    final originalPicker = FileSelectorPlatform.instance;
    FileSelectorPlatform.instance = _FilePicker([
      XFile.fromData(Uint8List.fromList([137, 80, 78, 71]), path: '/reply.png'),
    ]);
    addTearDown(() => FileSelectorPlatform.instance = originalPicker);
    final client = FakeHttpClient(defaultResponse: FakeHttpResponse());
    client.json('POST', '/users/alice/media', {'id': 'uploaded-image'});
    client.route('POST', '/users/alice/outbox',
        FakeHttpResponse(statusCode: 503, body: 'offline'));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    var sent = 0;
    await pumpScreen(
        tester,
        Scaffold(
            body: ReplyComposer(
          api: CoreApi(config: buildTestConfig()),
          inReplyTo: 'https://other.test/notes/parent',
          replyToActor: 'https://other.test/users/bob',
          onSent: () => sent++,
          onCancel: () {},
        )));
    await tester.enterText(find.byType(TextField), 'An image & a reply');
    await tester.tap(find.byTooltip('Attach media'));
    await tester.pumpAndSettle();
    expect(find.text('reply.png'), findsOneWidget);
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(sent, 0);
    expect(find.text('reply.png'), findsOneWidget);
    expect(find.text('An image & a reply'), findsOneWidget);
    client.json('POST', '/users/alice/outbox', {});
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(sent, 1,
        reason: tester
            .widgetList<Text>(find.byType(Text))
            .map((t) => t.data)
            .join(' | '));
    expect(client.requestsFor('POST', '/users/alice/media'), hasLength(1));
    final payload = jsonDecode(client.lastBody) as Map<String, dynamic>;
    final note = payload['object'] as Map;
    expect(note['inReplyTo'], 'https://other.test/notes/parent');
    expect(note['attachment'], ['uploaded-image']);
    expect(note['content'], contains('&amp;'));
    expect(payload['to'], contains('https://other.test/users/bob'));
  });

  testWidgets('media-only reply fits a narrow screen and removes attachments',
      (tester) async {
    await setUpPumpEnv(tester);
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(320, 760);
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final originalPicker = FileSelectorPlatform.instance;
    FileSelectorPlatform.instance = _FilePicker([
      XFile.fromData(Uint8List.fromList([1, 2, 3]), path: '/clip.mp4'),
    ]);
    addTearDown(() => FileSelectorPlatform.instance = originalPicker);
    final client = FakeHttpClient(defaultResponse: FakeHttpResponse());
    client.json('POST', '/users/alice/media', {'id': 'video'});
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    var sent = false;
    await pumpScreen(
        tester,
        Scaffold(
            body: ReplyComposer(
          api: CoreApi(config: buildTestConfig()),
          inReplyTo: 'https://other.test/notes/parent',
          replyToActor: 'https://other.test/users/bob',
          onSent: () => sent = true,
          onCancel: () {},
        )),
        textScale: 2);
    await tester.tap(find.byTooltip('Attach media'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Remove attachment'));
    await tester.pumpAndSettle();
    expect(find.text('clip.mp4'), findsNothing);
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(client.requests, isEmpty);
    await tester.tap(find.byTooltip('Attach media'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(sent, isTrue);
    expect((jsonDecode(client.lastBody)['object'] as Map)['attachment'],
        ['video']);
    expect(tester.takeException(), isNull);
  });

  testWidgets('failed upload never publishes a reply without its attachment',
      (tester) async {
    await setUpPumpEnv(tester);
    final originalPicker = FileSelectorPlatform.instance;
    FileSelectorPlatform.instance = _FilePicker([
      XFile.fromData(Uint8List.fromList([1]), path: '/voice.m4a'),
    ]);
    addTearDown(() => FileSelectorPlatform.instance = originalPicker);
    final client = FakeHttpClient(defaultResponse: FakeHttpResponse());
    client.route(
        'POST', '/users/alice/media', FakeHttpResponse(statusCode: 503));
    CoreHttp.useTestClient(client);
    addTearDown(CoreHttp.reset);
    await pumpScreen(
        tester,
        Scaffold(
            body: ReplyComposer(
          api: CoreApi(config: buildTestConfig()),
          inReplyTo: 'https://other.test/notes/parent',
          replyToActor: 'https://other.test/users/bob',
          onSent: () => fail('Failed upload must keep the reply draft'),
          onCancel: () {},
        )));
    await tester.enterText(find.byType(TextField), 'Keep this reply');
    await tester.tap(find.byTooltip('Attach media'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(client.requestsFor('POST', '/users/alice/outbox'), isEmpty);
    expect(find.text('voice.m4a'), findsOneWidget);
    expect(find.text('Keep this reply'), findsOneWidget);
  });
}
