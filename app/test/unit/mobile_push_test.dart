import 'dart:convert';
import 'dart:async';
import 'package:fedi3/services/mobile_push_service.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:fedi3/core/core_http.dart';
import '../support/fake_http.dart';
import '../support/fake_core.dart';
import '../support/test_config.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  tearDown(CoreHttp.reset);
  test('APNs native token renews relay registration and disable revokes it',
      () async {
    final cfg = buildTestConfig();
    final scope = await MobilePushService.scopeFor(cfg);
    FlutterSecureStorage.setMockInitialValues({
      'fedi3.mobilePush.v1': jsonEncode({'scope': scope, 'enabled': true})
    });
    const channel = MethodChannel('fedi3/test-push');
    final calls = <String>[];
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      calls.add(call.method);
      return call.method == 'initialize' ? true : null;
    });
    final client = FakeHttpClient();
    client.json('PUT', '/_fedi3/relay/push/alice', {'expires_at_ms': 1});
    client.route('DELETE', '/_fedi3/relay/push/alice',
        FakeHttpResponse(statusCode: 204));
    CoreHttp.useTestClient(client);
    final app = buildTestAppState(config: cfg);
    final service = MobilePushService.forTesting(
        android: false,
        ios: true,
        storage: const FlutterSecureStorage(),
        channel: channel);
    await service.initialize(app, background: true);
    Future<void> native(String method, dynamic argument) async {
      final done = Completer<void>();
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
              channel.name,
              const StandardMethodCodec()
                  .encodeMethodCall(MethodCall(method, argument)),
              (_) => done.complete());
      await done.future;
    }

    await native('token', 'a' * 64);
    expect(service.status.value, 'registered');
    expect(
        jsonDecode(client.lastBody), {'provider': 'apns', 'token': 'a' * 64});
    await service.disable();
    expect(client.requests.last.method, 'DELETE');
    expect(calls, contains('disable'));
    final count = client.requests.length;
    await native('token', 'b' * 64);
    expect(client.requests.length, count);
    app.dispose();
  });
  test('background wakeups coalesce and release the core they start', () async {
    final cfg = buildTestConfig();
    final scope = await MobilePushService.scopeFor(cfg);
    FlutterSecureStorage.setMockInitialValues({
      'fedi3.mobilePush.v1': jsonEncode({'scope': scope, 'enabled': true})
    });
    const channel = MethodChannel('fedi3/test-wake');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (_) async => true);
    final client = FakeHttpClient();
    var polls = 0;
    client.handler = (_) => FakeHttpResponse(
        body: jsonEncode({'pending_items': polls++ == 0 ? 1 : 0}));
    CoreHttp.useTestClient(client);
    final core = FakeCoreRuntime();
    final app = buildTestAppState(config: cfg, core: core);
    final service = MobilePushService.forTesting(
        android: false,
        ios: true,
        storage: const FlutterSecureStorage(),
        channel: channel);
    await service.initialize(app, background: true);
    await Future.wait([service.wake(), service.wake(), service.wake()]);
    expect(core.startCalls, 1);
    expect(core.stopCalls, 1);
    expect(app.isRunning, isFalse);
    app.dispose();
  });

  test('wakeups require bounded JSON with the expected event', () {
    final bytes = utf8.encode('{"event":"sync"}');
    expect(MobilePushService.validWake(bytes, decrypted: true), isTrue);
    expect(MobilePushService.validWake(bytes, decrypted: false), isTrue);
    expect(
        MobilePushService.validWake(
            utf8.encode('{"event":"chat","body":"spoof"}'),
            decrypted: true),
        isFalse);
    expect(MobilePushService.validWake(List.filled(4097, 0), decrypted: true),
        isFalse);
    expect(MobilePushService.validWake([255], decrypted: true), isFalse);
  });
  test('endpoint scope binds account and relay without exposing either',
      () async {
    final config = buildTestConfig().copyWith(relayWs: 'wss://one.example');
    final scope = await MobilePushService.scopeFor(config);
    expect(scope, hasLength(43));
    expect(scope, isNot(contains(config.username)));
    expect(await MobilePushService.scopeFor(config.copyWith(username: 'bob')),
        isNot(scope));
    expect(
        await MobilePushService.scopeFor(
            config.copyWith(relayWs: 'wss://two.example')),
        isNot(scope));
  });
}
