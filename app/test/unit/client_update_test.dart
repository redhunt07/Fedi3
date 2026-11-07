/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:fedi3/services/client_update_service.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_http.dart';

const _name = 'Fedi3-linux-x86_64.AppImage';
const _base = 'https://github.com/redhunt07/Fedi3';
const _tag = 'v0.10.1';
final _bytes = utf8.encode('verified AppImage content');
final _digest = sha256.convert(_bytes).toString();

Map<String, dynamic> _asset(String name, int size, {String? digest}) => {
      'name': name,
      'size': size,
      'state': 'uploaded',
      'browser_download_url': '$_base/releases/download/$_tag/$name',
      if (digest != null) 'digest': 'sha256:$digest',
    };

Map<String, dynamic> _release({bool checksums = false}) => {
      'draft': false,
      'prerelease': false,
      'tag_name': _tag,
      'html_url': '$_base/releases/tag/$_tag',
      'body': 'Release notes',
      'assets': [
        _asset(_name, _bytes.length, digest: checksums ? null : _digest),
        if (checksums) _asset('checksums.txt', 100),
      ],
    };

void main() {
  late FakeHttpClient client;
  late ClientUpdateService service;
  late Directory temp;

  setUp(() {
    client = FakeHttpClient();
    service = ClientUpdateService(client: client);
    temp = Directory.systemTemp.createTempSync('fedi3-updater-test-');
    client.json('GET', '/repos/redhunt07/Fedi3/releases/latest', _release());
    client.route('GET', '/redhunt07/Fedi3/releases/download/$_tag/$_name',
        FakeHttpResponse(body: utf8.decode(_bytes)));
  });
  tearDown(() => temp.deleteSync(recursive: true));

  Future<ClientUpdate?> check(
          {String current = '0.10.0', String arch = 'x64'}) =>
      service.check(currentVersion: current, os: 'linux', architecture: arch);

  test('compares numeric versions and never offers a downgrade', () async {
    expect((await check(current: '0.9.19'))!.available, isTrue);
    expect((await check(current: '0.10.1'))!.available, isFalse);
    expect((await check(current: '0.11.0'))!.available, isFalse);
    expect((await check())!.asset!.name, _name);
    expect(client.requests.every((r) => r.method == 'GET'), isTrue);
    for (final request in client.requests) {
      expect(request.headers.keys.map((k) => k.toLowerCase()),
          isNot(contains('authorization')));
      expect(request.headers.keys.map((k) => k.toLowerCase()),
          isNot(contains('x-fedi3-internal')));
    }
  });

  test('unsupported architectures get release information without x64 packages',
      () async {
    expect((await check(arch: 'arm64'))!.asset, isNull);
    expect(ClientUpdateService.assetName('windows', 'x64'),
        'Fedi3-windows-x64.zip');
    expect(ClientUpdateService.assetName('android', 'arm64'), isNull);
  });

  test('no published release is a valid empty result', () async {
    client.route('GET', '/repos/redhunt07/Fedi3/releases/latest',
        FakeHttpResponse(statusCode: 404));
    expect(await check(), isNull);
  });

  test('server and invalid JSON errors remain visible to manual checks',
      () async {
    client.route('GET', '/repos/redhunt07/Fedi3/releases/latest',
        FakeHttpResponse(statusCode: 429));
    await expectLater(check(), throwsA(isA<ClientUpdateException>()));
    client.route('GET', '/repos/redhunt07/Fedi3/releases/latest',
        FakeHttpResponse(body: 'not json'));
    await expectLater(check(), throwsFormatException);
  });

  for (final field in ['draft', 'prerelease']) {
    test('rejects $field releases', () async {
      client.json('GET', '/repos/redhunt07/Fedi3/releases/latest',
          _release()..[field] = true);
      await expectLater(check(), throwsA(isA<ClientUpdateException>()));
    });
  }

  test('rejects asset URLs outside the official tagged release', () async {
    final raw = _release();
    (raw['assets'] as List).first['browser_download_url'] =
        'https://evil.test/update';
    client.json('GET', '/repos/redhunt07/Fedi3/releases/latest', raw);
    await expectLater(check(), throwsA(isA<ClientUpdateException>()));
  });

  test('rejects release page spoofing, duplicate packages and unbounded sizes',
      () async {
    for (final raw in [
      _release()..['html_url'] = 'https://evil.test',
      _release()..['assets'] = [_asset(_name, 2), _asset(_name, 2)],
      _release()
        ..['assets'] = [_asset(_name, ClientUpdateService.maxPackageBytes + 1)],
    ]) {
      client.json('GET', '/repos/redhunt07/Fedi3/releases/latest', raw);
      await expectLater(check(), throwsA(isA<ClientUpdateException>()));
    }
  });

  test('download verifies digest and keeps only the completed package',
      () async {
    var received = 0;
    final package = await service.download((await check())!, temp,
        onProgress: (value, total) {
      received = value;
      expect(total, _bytes.length);
    });
    expect(received, _bytes.length);
    expect(await package.file.readAsBytes(), _bytes);
    expect(package.sha256, _digest);
    expect(package.file.path.endsWith('.part'), isFalse);
  });

  test('older releases can supply a checksums.txt entry', () async {
    client.json('GET', '/repos/redhunt07/Fedi3/releases/latest',
        _release(checksums: true));
    client.route(
        'GET',
        '/redhunt07/Fedi3/releases/download/$_tag/checksums.txt',
        FakeHttpResponse(body: '$_digest  $_name\n'));
    final package = await service.download((await check())!, temp);
    expect(await package.file.readAsBytes(), _bytes);
  });

  test('refuses missing or ambiguous checksums before downloading a package',
      () async {
    client.json('GET', '/repos/redhunt07/Fedi3/releases/latest',
        _release(checksums: true));
    final update = (await check())!;
    for (final body in ['invalid', '$_digest  $_name\n$_digest  $_name\n']) {
      client.route(
          'GET',
          '/redhunt07/Fedi3/releases/download/$_tag/checksums.txt',
          FakeHttpResponse(body: body));
      await expectLater(service.download(update, temp),
          throwsA(isA<ClientUpdateException>()));
    }
    expect(
        client.requestsFor(
            'GET', '/redhunt07/Fedi3/releases/download/$_tag/$_name'),
        isEmpty);
  });

  test(
      'checksum failure removes temporary files and preserves the installed app',
      () async {
    final target = File('${temp.path}/installed.AppImage')
      ..writeAsStringSync('old version');
    final raw = _release()
      ..['assets'] = [_asset(_name, _bytes.length, digest: '0' * 64)];
    client.json('GET', '/repos/redhunt07/Fedi3/releases/latest', raw);
    await expectLater(service.download((await check())!, temp),
        throwsA(isA<ClientUpdateException>()));
    expect(temp.listSync().map((f) => f.path), [target.path]);
    expect(target.readAsStringSync(), 'old version');
  });

  test('truncated downloads are rejected and cleaned up', () async {
    client.route('GET', '/redhunt07/Fedi3/releases/download/$_tag/$_name',
        FakeHttpResponse(body: 'short'));
    await expectLater(service.download((await check())!, temp),
        throwsA(isA<ClientUpdateException>()));
    expect(temp.listSync(), isEmpty);
  });

  test('installs AppImage with executable permissions and a previous copy',
      () async {
    final target = File('${temp.path}/installed.AppImage')
      ..writeAsStringSync('old version');
    final package = await service.download((await check())!, temp);
    await service.installAppImage(package, target.path);
    expect(await target.readAsBytes(), _bytes);
    expect(File('${target.path}.previous').readAsStringSync(), 'old version');
    expect((await target.stat()).mode & 0x49, 0x49);
  }, skip: !Platform.isLinux);

  test('rechecks staged package and leaves the current app intact on tampering',
      () async {
    final target = File('${temp.path}/installed.AppImage')
      ..writeAsStringSync('old version');
    final package = await service.download((await check())!, temp);
    await package.file.writeAsString('tampered');
    await expectLater(service.installAppImage(package, target.path),
        throwsA(isA<ClientUpdateException>()));
    expect(target.readAsStringSync(), 'old version');
    expect(File('${target.path}.previous').existsSync(), isFalse);
  }, skip: !Platform.isLinux);
}
