/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:http/http.dart' as http;
import 'package:package_info_plus/package_info_plus.dart';
import 'package:pub_semver/pub_semver.dart';

import '../core/core_http.dart';

class ClientUpdateException implements Exception {
  const ClientUpdateException(this.message);
  final String message;
  @override
  String toString() => message;
}

class ClientUpdateAsset {
  const ClientUpdateAsset(
      {required this.name, required this.url, required this.size, this.digest});
  final String name;
  final Uri url;
  final int size;
  final String? digest;
}

class ClientUpdate {
  const ClientUpdate(
      {required this.version,
      required this.currentVersion,
      required this.releaseUrl,
      required this.notes,
      required this.asset,
      required this.checksums});
  final Version version;
  final Version currentVersion;
  final Uri releaseUrl;
  final String notes;
  final ClientUpdateAsset? asset;
  final ClientUpdateAsset? checksums;
  bool get available => version > currentVersion;
}

/// Only [ClientUpdateService.download] can create a verified package.
class VerifiedClientUpdate {
  const VerifiedClientUpdate._(this.file, this.sha256, this.assetName);
  final File file;
  final String sha256;
  final String assetName;
}

/// Read-only GitHub Releases checks. Never sends credentials or telemetry.
class ClientUpdateService {
  ClientUpdateService({http.Client? client}) : _client = client;
  final http.Client? _client;
  static final latestUrl =
      Uri.parse('https://api.github.com/repos/redhunt07/Fedi3/releases/latest');
  static const repositoryUrl = 'https://github.com/redhunt07/Fedi3';
  static const maxPackageBytes = 1024 * 1024 * 1024;
  static const _timeout = Duration(seconds: 15);

  static String? assetName(String os, String architecture) {
    if (os == 'linux' && architecture == 'x64') {
      return 'Fedi3-linux-x86_64.AppImage';
    }
    if (os == 'windows' && architecture == 'x64') {
      return 'Fedi3-windows-x64.zip';
    }
    return null;
  }

  Future<ClientUpdate?> check(
      {String? currentVersion,
      required String os,
      required String architecture}) async {
    final local = Version.parse(
        currentVersion ?? (await PackageInfo.fromPlatform()).version);
    final bytes =
        await _read(latestUrl, maxBytes: 2 * 1024 * 1024, allowNotFound: true);
    if (bytes == null) return null; // No published releases yet.
    final raw = jsonDecode(utf8.decode(bytes));
    if (raw is! Map || raw['draft'] != false || raw['prerelease'] != false) {
      throw const ClientUpdateException('Invalid stable release metadata');
    }
    final tag = raw['tag_name'];
    if (tag is! String || !RegExp(r'^v\d+\.\d+\.\d+$').hasMatch(tag)) {
      throw const ClientUpdateException('Invalid release version');
    }
    final version = Version.parse(tag.substring(1));
    final expectedPage = Uri.parse('$repositoryUrl/releases/tag/$tag');
    if (raw['html_url'] != expectedPage.toString() || raw['assets'] is! List) {
      throw const ClientUpdateException('Invalid release location');
    }
    ClientUpdateAsset? find(String? name) {
      if (name == null) return null;
      final matches = (raw['assets'] as List)
          .whereType<Map>()
          .where((a) => a['name'] == name)
          .toList();
      if (matches.isEmpty) return null;
      if (matches.length != 1) {
        throw const ClientUpdateException('Duplicate release asset');
      }
      final a = matches.single;
      final url = Uri.parse('$repositoryUrl/releases/download/$tag/$name');
      final size = a['size'];
      if (a['browser_download_url'] != url.toString() ||
          size is! int ||
          size <= 0 ||
          size > maxPackageBytes ||
          a['state'] != 'uploaded') {
        throw const ClientUpdateException('Invalid release asset');
      }
      final digest = a['digest'];
      if (digest != null &&
          (digest is! String ||
              !RegExp(r'^sha256:[a-fA-F0-9]{64}$').hasMatch(digest))) {
        throw const ClientUpdateException('Invalid release checksum');
      }
      return ClientUpdateAsset(
          name: name,
          url: url,
          size: size,
          digest: digest is String ? digest.substring(7).toLowerCase() : null);
    }

    return ClientUpdate(
        version: version,
        currentVersion: local,
        releaseUrl: expectedPage,
        notes: raw['body'] is String ? raw['body'] : '',
        asset: find(assetName(os, architecture)),
        checksums: find('checksums.txt'));
  }

  Future<List<int>?> _read(Uri url,
      {required int maxBytes, bool allowNotFound = false}) async {
    final client = _client ?? CoreHttp.streamClient();
    final ownsClient = _client == null && CoreHttp.ownsStreamClient(client);
    try {
      return await (() async {
        final response = await client.send(http.Request('GET', url)
          ..headers['Accept'] = url.host == 'api.github.com'
              ? 'application/vnd.github+json'
              : 'application/octet-stream');
        if (allowNotFound && response.statusCode == 404) return null;
        if (response.statusCode != 200) {
          throw ClientUpdateException(
              'Update server returned HTTP ${response.statusCode}');
        }
        final bytes = <int>[];
        await for (final chunk in response.stream) {
          if (bytes.length + chunk.length > maxBytes) {
            throw const ClientUpdateException('Update metadata is too large');
          }
          bytes.addAll(chunk);
        }
        return bytes;
      })()
          .timeout(_timeout);
    } finally {
      if (ownsClient) client.close();
    }
  }

  Future<String> _checksum(ClientUpdate update, ClientUpdateAsset asset) async {
    if (asset.digest != null) return asset.digest!;
    final checksums = update.checksums;
    if (checksums == null) {
      throw const ClientUpdateException('Release has no SHA256 checksum');
    }
    final bytes = await _read(checksums.url, maxBytes: 64 * 1024);
    final matches = LineSplitter.split(utf8.decode(bytes!))
        .map((line) => RegExp(r'^([a-fA-F0-9]{64}) [ *](.+)$').firstMatch(line))
        .where((m) => m != null && m.group(2) == asset.name)
        .toList();
    if (matches.length != 1) {
      throw const ClientUpdateException(
          'Missing or ambiguous package checksum');
    }
    return matches.single!.group(1)!.toLowerCase();
  }

  Future<VerifiedClientUpdate> download(
      ClientUpdate update, Directory directory,
      {void Function(int received, int total)? onProgress}) async {
    final asset = update.asset;
    if (!update.available || asset == null) {
      throw const ClientUpdateException('No compatible newer package');
    }
    final expected = await _checksum(update, asset);
    await directory.create(recursive: true);
    final work = await directory.createTemp('fedi3-update-');
    final partial = File('${work.path}/${asset.name}.part');
    final completed = File('${work.path}/${asset.name}');
    final client = _client ?? CoreHttp.streamClient();
    final ownsClient = _client == null && CoreHttp.ownsStreamClient(client);
    try {
      await (() async {
        final response =
            await client.send(http.Request('GET', asset.url)).timeout(_timeout);
        if (response.statusCode != 200 ||
            (response.contentLength != null &&
                response.contentLength != asset.size)) {
          throw const ClientUpdateException('Invalid package response');
        }
        final sink = partial.openWrite();
        var received = 0;
        final elapsed = Stopwatch()..start();
        try {
          await for (final chunk
              in response.stream.timeout(const Duration(seconds: 30))) {
            if (elapsed.elapsed > const Duration(minutes: 15)) {
              throw const ClientUpdateException('Package download timed out');
            }
            received += chunk.length;
            if (received > asset.size || received > maxPackageBytes) {
              throw const ClientUpdateException(
                  'Package exceeds declared size');
            }
            sink.add(chunk);
            // Apply backpressure instead of buffering the entire package in memory.
            await sink.flush();
            onProgress?.call(received, asset.size);
          }
        } finally {
          await sink.close();
        }
        if (received != asset.size ||
            (await sha256.bind(partial.openRead()).first).toString() !=
                expected) {
          throw const ClientUpdateException('Package SHA256 or size mismatch');
        }
      })();
      await partial.rename(completed.path);
      return VerifiedClientUpdate._(completed, expected, asset.name);
    } catch (_) {
      // A failed download must never leave an installable package behind.
      if (await work.exists()) await work.delete(recursive: true);
      rethrow;
    } finally {
      if (ownsClient) client.close();
    }
  }

  /// Atomically replaces an AppImage; the running process stays on the old inode.
  /// The caller chooses when to restart. The previous image remains recoverable.
  Future<void> installAppImage(
      VerifiedClientUpdate package, String targetPath) async {
    if (!Platform.isLinux ||
        !package.assetName.endsWith('.AppImage') ||
        !targetPath.endsWith('.AppImage')) {
      throw const ClientUpdateException(
          'Only Linux AppImage replacement is supported');
    }
    final target = File(await File(targetPath).resolveSymbolicLinks());
    final work = await target.parent.createTemp('.fedi3-update-');
    try {
      final staged = await package.file.copy('${work.path}/new.AppImage');
      if ((await sha256.bind(staged.openRead()).first).toString() !=
          package.sha256) {
        throw const ClientUpdateException('Package changed after verification');
      }
      final chmod = await Process.run('chmod', ['755', staged.path]);
      if (chmod.exitCode != 0) {
        throw const ClientUpdateException('Cannot make the update executable');
      }
      final backup = await target.copy('${work.path}/previous.AppImage');
      await backup.rename('${target.path}.previous');
      await staged.rename(target.path);
    } finally {
      await work.delete(recursive: true);
    }
  }
}
