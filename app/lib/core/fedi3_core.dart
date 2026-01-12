/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:ffi' as ffi;
import 'dart:io' show Directory, File, Platform;

import 'package:ffi/ffi.dart';

typedef _VersionNative = ffi.Pointer<Utf8> Function();
typedef _VersionDart = ffi.Pointer<Utf8> Function();

typedef _StringFreeNative = ffi.Void Function(ffi.Pointer<ffi.Char>);
typedef _StringFreeDart = void Function(ffi.Pointer<ffi.Char>);

typedef _StartNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Uint64>,
  ffi.Pointer<ffi.Pointer<ffi.Char>>,
);
typedef _StartDart = int Function(
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Uint64>,
  ffi.Pointer<ffi.Pointer<ffi.Char>>,
);

typedef _StopNative = ffi.Int32 Function(ffi.Uint64, ffi.Pointer<ffi.Pointer<ffi.Char>>);
typedef _StopDart = int Function(int, ffi.Pointer<ffi.Pointer<ffi.Char>>);

ffi.DynamicLibrary _openCoreLibrary() {
  if (Platform.isAndroid) return ffi.DynamicLibrary.open('libfedi3_core.so');
  if (Platform.isIOS) return ffi.DynamicLibrary.process();

  final exeDir = File(Platform.resolvedExecutable).parent.path;

  if (Platform.isWindows) {
    return _openFirstExisting(_desktopCandidates(
      exeDir: exeDir,
      fileName: 'fedi3_core.dll',
      parentSearchDepth: 8,
    ));
  }

  if (Platform.isLinux) {
    return _openFirstExisting(_desktopCandidates(
      exeDir: exeDir,
      fileName: 'libfedi3_core.so',
      parentSearchDepth: 6,
    ));
  }

  if (Platform.isMacOS) {
    return _openFirstExisting(_desktopCandidates(
      exeDir: exeDir,
      fileName: 'libfedi3_core.dylib',
      parentSearchDepth: 6,
    ));
  }

  throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
}

List<String> _desktopCandidates({
  required String exeDir,
  required String fileName,
  required int parentSearchDepth,
}) {
  final candidates = <String>[];

  candidates.add(_joinPath(exeDir, fileName));
  candidates.add(_joinPath(_joinPath(exeDir, 'lib'), fileName));
  candidates.add(fileName);

  var dir = Directory(exeDir);
  for (var i = 0; i < parentSearchDepth; i++) {
    final parent = dir.parent;
    if (parent.path == dir.path) break;
    dir = parent;
    candidates.add(_joinPath(dir.path, fileName));
    candidates.add(_joinPath(_joinPath(dir.path, 'lib'), fileName));
  }

  return candidates;
}

String _joinPath(String dir, String fileName) {
  if (Platform.isWindows) return '$dir\\$fileName';
  return '$dir/$fileName';
}

ffi.DynamicLibrary _openFirstExisting(List<String> candidates) {
  Object? lastError;
  for (final candidate in candidates) {
    try {
      return ffi.DynamicLibrary.open(candidate);
    } catch (e) {
      lastError = e;
    }
  }
  throw StateError('Unable to load fedi3_core library. Tried: $candidates. Last error: $lastError');
}

class Fedi3Core {
  Fedi3Core._(this._lib)
      : _version = _lib.lookupFunction<_VersionNative, _VersionDart>('fedi3_core_version'),
        _stringFree = _lib.lookupFunction<_StringFreeNative, _StringFreeDart>(
          'fedi3_core_string_free',
        );

  final ffi.DynamicLibrary _lib;
  final _VersionDart _version;
  final _StringFreeDart _stringFree;
  late final _StartDart _start = _lib.lookupFunction<_StartNative, _StartDart>('fedi3_core_start');
  late final _StopDart _stop = _lib.lookupFunction<_StopNative, _StopDart>('fedi3_core_stop');
  late final _StringFreeDart _freeCString =
      _lib.lookupFunction<_StringFreeNative, _StringFreeDart>('fedi3_core_free_cstring');

  static final Fedi3Core instance = Fedi3Core._(_openCoreLibrary());

  String version() {
    final ptr = _version();
    try {
      return ptr.toDartString();
    } finally {
      _stringFree(ptr.cast<ffi.Char>());
    }
  }

  int startJson(String configJson) {
    final cfgPtr = configJson.toNativeUtf8().cast<ffi.Char>();
    final outHandle = calloc<ffi.Uint64>();
    final outErr = calloc<ffi.Pointer<ffi.Char>>();
    try {
      final rc = _start(cfgPtr, outHandle, outErr);
      if (rc != 0) {
        final errPtr = outErr.value;
        final msg = errPtr == ffi.nullptr ? 'unknown error (rc=$rc)' : errPtr.cast<Utf8>().toDartString();
        if (errPtr != ffi.nullptr) _freeCString(errPtr);
        throw StateError('core start failed: $msg');
      }
      return outHandle.value;
    } finally {
      malloc.free(cfgPtr);
      calloc.free(outHandle);
      calloc.free(outErr);
    }
  }

  void stop(int handle) {
    final outErr = calloc<ffi.Pointer<ffi.Char>>();
    try {
      final rc = _stop(handle, outErr);
      if (rc != 0) {
        final errPtr = outErr.value;
        final msg = errPtr == ffi.nullptr ? 'unknown error (rc=$rc)' : errPtr.cast<Utf8>().toDartString();
        if (errPtr != ffi.nullptr) _freeCString(errPtr);
        throw StateError('core stop failed: $msg');
      }
    } finally {
      calloc.free(outErr);
    }
  }
}
