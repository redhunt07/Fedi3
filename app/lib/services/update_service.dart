/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:archive/archive_io.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:path_provider/path_provider.dart';
import 'package:package_info_plus/package_info_plus.dart';

class UpdateInfo {
  UpdateInfo({
    required this.version,
    required this.assetName,
    required this.assetUrl,
    required this.checksum,
    required this.releasePage,
  });

  final String version;
  final String assetName;
  final String assetUrl;
  final String checksum;
  final String releasePage;
}

class UpdateService {
  UpdateService._();

  static final UpdateService instance = UpdateService._();

  static const _repo = 'redhunt07/Fedi3';
  static const _apiBase = 'https://api.github.com/repos/$_repo/releases/latest';
  static const _linuxAssetHint = 'linux';
  static const _winAssetHint = 'windows';
  static const _linuxAssetName = 'Fedi3-linux-x86_64.AppImage';
  static const _winAssetName = 'Fedi3-windows-x64.zip';
  static const _winExeName = 'Fedi3.exe';

  final ValueNotifier<UpdateInfo?> available = ValueNotifier<UpdateInfo?>(null);
  Timer? _timer;
  static const _versionMarkerName = 'fedi3_version.txt';

  Future<void> start() async {
    await checkNow();
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(hours: 12), (_) => checkNow());
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  Future<void> checkNow() async {
    if (!kReleaseMode) {
      available.value = null;
      return;
    }
    try {
      final info = await _fetchLatest();
      available.value = info;
    } catch (_) {
      // Best-effort: keep current state on failure.
    }
  }

  Future<void> downloadAndInstall() async {
    final info = available.value;
    if (info == null) return;
    final tmp = await getTemporaryDirectory();
    final targetPath = _currentExecutablePath();
    if (targetPath == null) {
      throw StateError('update: cannot resolve app path');
    }
    final fileName = info.assetName;
    final downloadPath = '${tmp.path}${Platform.pathSeparator}$fileName';
    final resp = await http.get(Uri.parse(info.assetUrl));
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('update download failed: ${resp.statusCode}');
    }
    final bytes = resp.bodyBytes;
    final digest = sha256.convert(bytes).toString();
    if (!digest.equalsIgnoreCase(info.checksum)) {
      throw StateError('update checksum mismatch');
    }
    final file = File(downloadPath);
    await file.writeAsBytes(bytes, flush: true);
    if (Platform.isWindows) {
      await _installWindows(downloadPath, targetPath, info.version);
    } else if (Platform.isLinux) {
      await _installLinux(downloadPath, targetPath, info.version);
    } else {
      throw StateError('update not supported on this platform');
    }
  }

  Future<UpdateInfo?> _fetchLatest() async {
    final resp = await http.get(Uri.parse(_apiBase), headers: {'User-Agent': 'fedi3-client'});
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('update check failed: ${resp.statusCode}');
    }
    final raw = jsonDecode(resp.body);
    if (raw is! Map<String, dynamic>) {
      throw StateError('invalid release payload');
    }
    final tag = (raw['tag_name'] as String?)?.trim() ?? '';
    if (tag.isEmpty) return null;
    final latestVersion = tag.startsWith('v') ? tag.substring(1) : tag;
    final current = await _currentVersion();
    if (!_isNewer(latestVersion, current)) return null;
    final assets = (raw['assets'] as List?) ?? const [];
    final releasePage = (raw['html_url'] as String?)?.trim() ?? '';
    final asset = _pickAsset(assets);
    if (asset == null) return null;
    final checksums = _findChecksumAsset(assets);
    if (checksums == null) {
      throw StateError('checksums.txt missing in release');
    }
    final checksumMap = await _loadChecksums(checksums);
    final checksum = checksumMap[asset['name']] ?? '';
    if (checksum.isEmpty) {
      throw StateError('checksum not found for asset ${asset['name']}');
    }
    return UpdateInfo(
      version: latestVersion,
      assetName: asset['name'] as String,
      assetUrl: asset['url'] as String,
      checksum: checksum,
      releasePage: releasePage,
    );
  }

  Future<String> _currentVersion() async {
    final info = await PackageInfo.fromPlatform();
    final v = info.version.trim();
    final b = info.buildNumber.trim();
    final pkgVersion = b.isEmpty ? v : '$v+$b';
    final marker = _readMarkerVersion();
    if (marker.isNotEmpty && !_isNewer(pkgVersion, marker)) {
      return marker;
    }
    return pkgVersion;
  }

  Map<String, dynamic>? _pickAsset(List assets) {
    final expectedName = Platform.isWindows ? _winAssetName : _linuxAssetName;
    for (final raw in assets) {
      if (raw is! Map) continue;
      final name = (raw['name'] as String?)?.trim() ?? '';
      if (name == expectedName) {
        final url = raw['browser_download_url'] as String?;
        if (url == null || url.isEmpty) return null;
        return {'name': name, 'url': url};
      }
    }
    if (Platform.isWindows) {
      for (final raw in assets) {
        if (raw is! Map) continue;
        final name = (raw['name'] as String?)?.trim() ?? '';
        if (name == _winExeName || name.toLowerCase().endsWith('.exe')) {
          final url = raw['browser_download_url'] as String?;
          if (url == null || url.isEmpty) return null;
          return {'name': name, 'url': url};
        }
      }
    }
    final platformHint = Platform.isWindows ? _winAssetHint : _linuxAssetHint;
    final candidates = assets.whereType<Map>().where((a) {
      final name = (a['name'] as String?)?.toLowerCase() ?? '';
      if (Platform.isWindows && name.endsWith('.exe')) return true;
      if (Platform.isLinux && name.endsWith('.appimage')) return true;
      return name.contains(platformHint);
    }).toList();
    if (candidates.isEmpty) return null;
    candidates.sort((a, b) {
      final an = (a['name'] as String?)?.length ?? 0;
      final bn = (b['name'] as String?)?.length ?? 0;
      return an.compareTo(bn);
    });
    final best = candidates.first;
    final url = best['browser_download_url'] as String?;
    if (url == null || url.isEmpty) return null;
    return {
      'name': best['name'],
      'url': url,
    };
  }

  Map<String, dynamic>? _findChecksumAsset(List assets) {
    for (final raw in assets) {
      if (raw is! Map) continue;
      final name = (raw['name'] as String?)?.toLowerCase() ?? '';
      if (name == 'checksums.txt' || name == 'sha256sums.txt') {
        final url = raw['browser_download_url'] as String?;
        if (url == null || url.isEmpty) continue;
        return {'name': raw['name'], 'url': url};
      }
    }
    return null;
  }

  Future<Map<String, String>> _loadChecksums(Map<String, dynamic> asset) async {
    final url = asset['url'] as String;
    final resp = await http.get(Uri.parse(url));
    if (resp.statusCode < 200 || resp.statusCode >= 300) {
      throw StateError('checksums download failed: ${resp.statusCode}');
    }
    final out = <String, String>{};
    for (final line in const LineSplitter().convert(resp.body)) {
      final parts = line.trim().split(RegExp(r'\s+'));
      if (parts.length < 2) continue;
      final checksum = parts.first.trim();
      final name = parts.last.trim();
      if (checksum.length >= 32 && name.isNotEmpty) {
        out[name] = checksum;
      }
    }
    return out;
  }

  bool _isNewer(String latest, String current) {
    final l = _parseVersion(latest);
    final c = _parseVersion(current);
    for (var i = 0; i < l.length; i++) {
      if (i >= c.length) return true;
      if (l[i] > c[i]) return true;
      if (l[i] < c[i]) return false;
    }
    return l.length > c.length;
  }

  List<int> _parseVersion(String v) {
    final raw = v.split('+').first.trim();
    return raw
        .split('.')
        .map((part) => int.tryParse(part.replaceAll(RegExp(r'[^0-9]'), '')) ?? 0)
        .toList();
  }

  String? _currentExecutablePath() {
    try {
      if (Platform.isLinux) {
        final appImage = Platform.environment['APPIMAGE']?.trim();
        if (appImage != null && appImage.isNotEmpty && File(appImage).existsSync()) {
          return appImage;
        }
      }
      return Platform.resolvedExecutable;
    } catch (_) {
      return null;
    }
  }

  Future<void> _installWindows(String downloadPath, String targetPath, String version) async {
    final tmp = await getTemporaryDirectory();
    var sourcePath = downloadPath;
    final targetDir = File(targetPath).parent.path;
    var copyDir = false;
    if (downloadPath.toLowerCase().endsWith('.zip')) {
      sourcePath = await _extractWindowsZip(downloadPath, tmp.path);
      copyDir = true;
    }
    final batPath = '${tmp.path}${Platform.pathSeparator}fedi3_update.bat';
    final script = StringBuffer()
      ..writeln('@echo off')
      ..writeln('ping 127.0.0.1 -n 2 > nul')
      ..writeln(copyDir
          ? 'xcopy /E /I /Y "${sourcePath}\\*" "${targetDir}\\" > nul'
          : 'copy /Y "${sourcePath}" "${targetPath}" > nul')
      ..writeln('echo ${version} > "${targetDir}\\${_versionMarkerName}"')
      ..writeln('start "" "${targetPath}"')
      ..writeln('del "%~f0"');
    await File(batPath).writeAsString(script.toString(), flush: true);
    await Process.start('cmd', ['/c', batPath], mode: ProcessStartMode.detached);
    exit(0);
  }

  Future<void> _installLinux(String downloadPath, String targetPath, String version) async {
    final tmp = await getTemporaryDirectory();
    final shPath = '${tmp.path}${Platform.pathSeparator}fedi3_update.sh';
    final targetDir = File(targetPath).parent.path;
    final script = StringBuffer()
      ..writeln('#!/bin/sh')
      ..writeln('sleep 1')
      ..writeln('mv -f "${downloadPath}" "${targetPath}"')
      ..writeln('chmod +x "${targetPath}"')
      ..writeln('printf "%s" "${version}" > "${targetDir}/${_versionMarkerName}"')
      ..writeln('"${targetPath}" &')
      ..writeln(r'rm -- "$0"');
    final file = File(shPath);
    await file.writeAsString(script.toString(), flush: true);
    await Process.run('chmod', ['+x', shPath]);
    await Process.start('/bin/sh', [shPath], mode: ProcessStartMode.detached);
    exit(0);
  }

  Future<String> _extractWindowsZip(String zipPath, String tempDir) async {
    final input = InputFileStream(zipPath);
    final archive = ZipDecoder().decodeBuffer(input);
    final rootPrefix = _detectZipRoot(archive);
    final outDir = Directory('${tempDir}${Platform.pathSeparator}fedi3_update');
    if (!await outDir.exists()) {
      await outDir.create(recursive: true);
    }
    for (final file in archive) {
      if (!file.isFile) continue;
      final name = file.name.replaceAll('\\', '/');
      var relPath = name.startsWith('/') ? name.substring(1) : name;
      if (rootPrefix != null && relPath.startsWith(rootPrefix)) {
        relPath = relPath.substring(rootPrefix.length);
      }
      if (relPath.startsWith('/')) relPath = relPath.substring(1);
      if (relPath.isEmpty) continue;
      final outPath = '${outDir.path}${Platform.pathSeparator}$relPath';
      final outFile = File(outPath);
      await outFile.parent.create(recursive: true);
      await outFile.writeAsBytes(file.content as List<int>, flush: true);
    }
    return outDir.path;
  }

  String? _detectZipRoot(Archive archive) {
    String? root;
    for (final file in archive) {
      final name = file.name.replaceAll('\\', '/').replaceAll(RegExp(r'^/+'), '');
      if (name.isEmpty) continue;
      final idx = name.indexOf('/');
      if (idx <= 0) {
        return null;
      }
      final prefix = name.substring(0, idx + 1);
      root ??= prefix;
      if (root != prefix) return null;
    }
    return root;
  }

  String _readMarkerVersion() {
    try {
      final exePath = _currentExecutablePath();
      if (exePath == null) return '';
      final dir = File(exePath).parent.path;
      final marker = File('${dir}${Platform.pathSeparator}${_versionMarkerName}');
      if (!marker.existsSync()) return '';
      return marker.readAsStringSync().trim();
    } catch (_) {
      return '';
    }
  }
}

extension on String {
  bool equalsIgnoreCase(String other) => toLowerCase() == other.toLowerCase();
}
