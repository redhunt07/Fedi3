/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:http/http.dart' as http;

import '../core/core_http.dart';
import '../model/relay_directory.dart';
import '../state/relay_directory_store.dart';

/// Why a relay could not be reached during the latency probe.
enum RelayProbeFailure {
  timeout,
  tlsHostMismatch,
  tlsCertificate,
  httpStatus,
  network,
}

/// A live latency measurement (or failure) for one directory relay.
class RelayLatency {
  const RelayLatency({
    required this.entry,
    required this.latencyMs,
    required this.failure,
    required this.measuredAt,
  });

  final RelayDirectoryEntry entry;

  /// Round-trip time to `GET /healthz`, when reachable.
  final int? latencyMs;

  /// Failure reason when [latencyMs] is null.
  final RelayProbeFailure? failure;

  final DateTime measuredAt;

  bool get reachable => latencyMs != null;
}

/// Fetches the shared relay directory and measures relay latency locally.
///
/// Nothing here is trusted blindly: the directory only advertises relays, and
/// every advertised relay is probed from the user's device before being shown
/// in the ranking.
class RelayDirectoryService {
  RelayDirectoryService._();

  /// Maximum time allowed to download the directory document.
  static const Duration fetchTimeout = Duration(seconds: 5);

  /// Maximum time allowed for a single latency probe.
  static const Duration probeTimeout = Duration(seconds: 4);

  /// Number of relays probed concurrently.
  static const int probeConcurrency = 6;

  /// Downloads and parses the directory, trying [sources] in order.
  static Future<RelayDirectorySnapshot> fetch({
    List<String>? sources,
    http.Client? client,
    Duration timeout = fetchTimeout,
  }) async {
    final candidates = (sources ?? RelayDirectory.sourceUrls)
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList(growable: false);
    if (candidates.isEmpty) {
      throw const RelayDirectoryFormatException(
          'no directory source configured');
    }
    final httpClient = client ?? CoreHttp.client;
    Object? lastError;
    for (final source in candidates) {
      try {
        final resp = await httpClient.get(Uri.parse(source)).timeout(timeout);
        if (resp.statusCode < 200 || resp.statusCode >= 300) {
          lastError = StateError('HTTP ${resp.statusCode} from $source');
          continue;
        }
        final raw = _decodeBody(resp);
        final entries = parseRelayDirectory(raw);
        if (entries.isEmpty) {
          lastError = StateError('empty relay directory from $source');
          continue;
        }
        final snapshot = RelayDirectorySnapshot(
          entries: entries,
          sourceUrl: source,
          fetchedAt: DateTime.now(),
          updatedAt: relayDirectoryUpdatedAt(raw),
        );
        await RelayDirectoryStore.cache(raw, sourceUrl: source);
        return snapshot;
      } catch (e) {
        lastError = e;
      }
    }
    throw StateError(
        'relay directory unavailable: ${lastError ?? 'unknown error'}');
  }

  /// Returns the last cached directory, if any.
  static Future<RelayDirectorySnapshot?> cached() async {
    final raw = await RelayDirectoryStore.cachedRaw();
    if (raw == null) return null;
    try {
      final entries = parseRelayDirectory(raw);
      if (entries.isEmpty) return null;
      return RelayDirectorySnapshot(
        entries: entries,
        sourceUrl: await RelayDirectoryStore.cachedSourceUrl() ??
            RelayDirectory.sourceUrls.first,
        fetchedAt: DateTime.now(),
        updatedAt: relayDirectoryUpdatedAt(raw),
      );
    } catch (_) {
      return null;
    }
  }

  /// Last-resort fallback: the directory copy bundled with the app. It is
  /// generated from `relays/relays.json` at release time, so a fresh install
  /// always has at least the relays known when the build was produced.
  static Future<RelayDirectorySnapshot?> bundled() async {
    try {
      final raw = await rootBundle.loadString('assets/relays.json');
      final entries = parseRelayDirectory(raw);
      if (entries.isEmpty) return null;
      return RelayDirectorySnapshot(
        entries: entries,
        sourceUrl: 'asset:assets/relays.json',
        fetchedAt: DateTime.now(),
        updatedAt: relayDirectoryUpdatedAt(raw),
      );
    } catch (_) {
      return null;
    }
  }

  /// Returns the freshest locally available directory: the cached copy when
  /// it is newer than the bundled asset, otherwise the bundled copy.
  static Future<RelayDirectorySnapshot?> cachedOrBundled() async {
    final bundled = await RelayDirectoryService.bundled();
    final cached = await RelayDirectoryService.cached();
    if (cached == null) return bundled;
    if (bundled == null) return cached;
    final cachedAt = DateTime.tryParse(cached.updatedAt ?? '');
    final bundledAt = DateTime.tryParse(bundled.updatedAt ?? '');
    if (cachedAt == null) return bundled;
    if (bundledAt == null) return cached;
    return cachedAt.isAfter(bundledAt) ? cached : bundled;
  }

  /// Probes every relay and returns them ranked from fastest to slowest.
  /// Unreachable relays are kept at the end so users still see them.
  static Future<List<RelayLatency>> probeAll(
    List<RelayDirectoryEntry> entries, {
    http.Client? client,
    Duration timeout = probeTimeout,
    int concurrency = probeConcurrency,
  }) async {
    if (entries.isEmpty) return const [];
    final httpClient = client ?? CoreHttp.client;
    final results = <RelayLatency>[];
    var next = 0;
    Future<void> worker() async {
      while (true) {
        final index = next++;
        if (index >= entries.length) return;
        results.add(await probeOne(entries[index],
            client: httpClient, timeout: timeout));
      }
    }

    final workers = math.min(concurrency, entries.length);
    await Future.wait(List.generate(workers, (_) => worker()));
    return rank(results);
  }

  /// Measures one relay with `GET {url}/healthz`.
  static Future<RelayLatency> probeOne(
    RelayDirectoryEntry entry, {
    http.Client? client,
    Duration timeout = probeTimeout,
  }) async {
    final httpClient = client ?? CoreHttp.client;
    final stopwatch = Stopwatch()..start();
    try {
      final resp = await httpClient
          .get(Uri.parse('${entry.url}/healthz'))
          .timeout(timeout);
      stopwatch.stop();
      final reachable = resp.statusCode >= 200 && resp.statusCode < 300;
      return RelayLatency(
        entry: entry,
        latencyMs: reachable ? stopwatch.elapsedMilliseconds : null,
        failure: reachable ? null : RelayProbeFailure.httpStatus,
        measuredAt: DateTime.now(),
      );
    } catch (e) {
      stopwatch.stop();
      return RelayLatency(
        entry: entry,
        latencyMs: null,
        failure: classifyProbeFailure(e),
        measuredAt: DateTime.now(),
      );
    }
  }

  /// Ranks probes: reachable relays by ascending latency, then failures.
  static List<RelayLatency> rank(List<RelayLatency> probes) {
    final sorted = List<RelayLatency>.from(probes);
    sorted.sort((a, b) {
      final aReachable = a.reachable;
      final bReachable = b.reachable;
      if (aReachable != bReachable) return aReachable ? -1 : 1;
      if (aReachable && bReachable) {
        return (a.latencyMs ?? 0).compareTo(b.latencyMs ?? 0);
      }
      return (a.failure?.index ?? 0).compareTo(b.failure?.index ?? 0);
    });
    return sorted;
  }

  /// Classifies a probe exception into a user-facing failure reason.
  static RelayProbeFailure classifyProbeFailure(Object error) {
    if (error is TimeoutException) return RelayProbeFailure.timeout;
    final text = error.toString().toUpperCase();
    if (text.contains('UNRECOGNIZED_NAME') ||
        text.contains('UNRECOGNIZED NAME') ||
        text.contains('WRONG_VERSION_NUMBER') ||
        text.contains('SSLV3_ALERT_HANDSHAKE_FAILURE')) {
      return RelayProbeFailure.tlsHostMismatch;
    }
    if (text.contains('CERTIFICATE_VERIFY_FAILED') ||
        text.contains('CERTIFICATE')) {
      return RelayProbeFailure.tlsCertificate;
    }
    if (error is HandshakeException) {
      return RelayProbeFailure.tlsCertificate;
    }
    return RelayProbeFailure.network;
  }

  static String _decodeBody(http.Response resp) {
    try {
      return utf8.decode(resp.bodyBytes);
    } catch (_) {
      return resp.body;
    }
  }
}
