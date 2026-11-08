/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

const defaultCallIceUrls = [
  'stun:stun.cloudflare.com:3478',
  'stun:stun.l.google.com:19302',
];

/// Default coturn listening port used when deriving relay TURN endpoints.
const defaultCallTurnPort = 3478;

bool _isTurnUrl(String url) {
  final lower = url.toLowerCase();
  return lower.startsWith('turn:') || lower.startsWith('turns:');
}

/// Extracts the relay host that also serves TURN.
///
/// Operators deploy coturn next to the relay, so the websocket host is the
/// correct TURN host. Local development URLs still produce a harmless host.
String? callRelayHost(String relayWs, String publicBaseUrl) {
  for (final raw in [relayWs, publicBaseUrl]) {
    final uri = Uri.tryParse(raw.trim());
    if (uri != null && uri.host.isNotEmpty) return uri.host;
  }
  return null;
}

/// Derives coturn endpoints for a relay host.
///
/// UDP is preferred for latency; the TCP transport is added because some
/// networks (captive portals, corporate VPNs) drop UDP entirely.
List<String> deriveCallTurnUrls(String host, {int port = defaultCallTurnPort}) {
  final trimmed = host.trim();
  if (trimmed.isEmpty) return const [];
  final effectivePort = port > 0 ? port : defaultCallTurnPort;
  return [
    'turn:$trimmed:$effectivePort?transport=udp',
    'turn:$trimmed:$effectivePort?transport=tcp',
  ];
}

/// Builds the ICE server list used by call peer connections.
///
/// STUN only discovers addresses and cannot relay media: when both peers sit
/// behind a VPN or carrier-grade NAT, ICE needs TURN. Operator credentials
/// alone used to be ignored unless the user also typed a `turn:` URL, so calls
/// failed with "network problem" while signaling worked. Now configured
/// credentials plus a relay host automatically produce TURN endpoints, and the
/// relay-provisioned pool (fetched separately) is appended by the call screen.
///
/// Keep one URL per ICE server: the Linux native WebRTC bridge retains only the
/// final URL when a server's `urls` value is a list.
List<Map<String, dynamic>> buildCallIceServers(
  List<String> urls, {
  String? username,
  String? credential,
  String? relayHost,
  int turnPort = defaultCallTurnPort,
}) {
  final configured = urls
      .map((url) => url.trim())
      .where((url) => url.isNotEmpty)
      .toSet()
      .toList(growable: true);
  final hasTurn = configured.any(_isTurnUrl);
  final usernameValue = username?.trim() ?? '';
  final credentialValue = credential?.trim() ?? '';
  final hasCredentials = usernameValue.isNotEmpty && credentialValue.isNotEmpty;
  if (!hasTurn && hasCredentials && relayHost != null) {
    for (final url in deriveCallTurnUrls(relayHost, port: turnPort)) {
      if (!configured.contains(url)) configured.add(url);
    }
  }
  final effective = configured.isEmpty ? defaultCallIceUrls : configured;
  return effective.map((url) {
    final turn = _isTurnUrl(url);
    return <String, dynamic>{
      'urls': url,
      if (turn && hasCredentials) 'username': usernameValue,
      if (turn && hasCredentials) 'credential': credentialValue,
    };
  }).toList(growable: false);
}
