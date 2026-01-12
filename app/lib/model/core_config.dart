/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:math';

class ProfileFieldKV {
  ProfileFieldKV({required this.name, required this.value});

  final String name;
  final String value;

  factory ProfileFieldKV.fromJson(Map<String, dynamic> json) => ProfileFieldKV(
        name: (json['name'] as String? ?? '').trim(),
        value: (json['value'] as String? ?? '').trim(),
      );

  Map<String, dynamic> toJson() => {'name': name, 'value': value};
}

class CoreConfig {
  CoreConfig({
    required this.username,
    required this.domain,
    required this.publicBaseUrl,
    required this.relayWs,
    required this.relayToken,
    required this.bind,
    required this.internalToken,
    required this.p2pEnable,
    required this.apRelays,
    required this.displayName,
    required this.summary,
    required this.iconUrl,
    required this.iconMediaType,
    required this.imageUrl,
    required this.imageMediaType,
    required this.profileFields,
    required this.manuallyApprovesFollowers,
    required this.blockedDomains,
    required this.blockedActors,
    this.postDeliveryMode = postDeliveryP2pRelay,
    this.p2pRelayReserve = const [],
    this.webrtcEnable = false,
    this.webrtcIceUrls = const [],
    this.webrtcIceUsername,
    this.webrtcIceCredential,
    this.p2pCacheTtlSecs,
    this.previousPublicBaseUrl,
    this.previousRelayToken,
  });

  final String username;
  final String domain;
  final String publicBaseUrl;
  final String relayWs;
  final String relayToken;
  final String bind;
  final String internalToken;
  final bool p2pEnable;
  final List<String> p2pRelayReserve;
  final bool webrtcEnable;
  final List<String> webrtcIceUrls;
  final String? webrtcIceUsername;
  final String? webrtcIceCredential;
  final List<String> apRelays;
  final String displayName;
  final String summary;
  final String iconUrl;
  final String iconMediaType;
  final String imageUrl;
  final String imageMediaType;
  final List<ProfileFieldKV> profileFields;
  final bool manuallyApprovesFollowers;
  final List<String> blockedDomains;
  final List<String> blockedActors;
  final String postDeliveryMode;
  final int? p2pCacheTtlSecs;
  final String? previousPublicBaseUrl;
  final String? previousRelayToken;

  Uri get localBaseUri => Uri.parse('http://$bind');

  static const String postDeliveryP2pRelay = 'p2p_relay';
  static const String postDeliveryP2pOnly = 'p2p_only';

  static CoreConfig fromJson(Map<String, dynamic> json) {
    final rawMode = (json['postDeliveryMode'] as String?)?.trim().toLowerCase();
    final postDeliveryMode = (rawMode == postDeliveryP2pOnly || rawMode == postDeliveryP2pRelay)
        ? rawMode!
        : postDeliveryP2pRelay;
    return CoreConfig(
      username: (json['username'] as String? ?? '').trim(),
      domain: (json['domain'] as String? ?? '').trim(),
      publicBaseUrl: (json['publicBaseUrl'] as String? ?? '').trim(),
      relayWs: (json['relayWs'] as String? ?? '').trim(),
      relayToken: (json['relayToken'] as String? ?? '').trim(),
      bind: (json['bind'] as String? ?? '').trim(),
      internalToken: (json['internalToken'] as String? ?? '').trim(),
      p2pEnable: (json['p2pEnable'] as bool?) ?? false,
      p2pRelayReserve: (json['p2pRelayReserve'] as List<dynamic>? ?? [])
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      webrtcEnable: (json['webrtcEnable'] as bool?) ?? ((json['p2pEnable'] as bool?) ?? false),
      webrtcIceUrls: (json['webrtcIceUrls'] as List<dynamic>? ?? [])
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      webrtcIceUsername: (json['webrtcIceUsername'] as String?)?.trim(),
      webrtcIceCredential: (json['webrtcIceCredential'] as String?)?.trim(),
      apRelays: (json['apRelays'] as List<dynamic>? ?? [])
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      displayName: (json['displayName'] as String? ?? '').trim(),
      summary: (json['summary'] as String? ?? '').trim(),
      iconUrl: (json['iconUrl'] as String? ?? '').trim(),
      iconMediaType: (json['iconMediaType'] as String? ?? '').trim(),
      imageUrl: (json['imageUrl'] as String? ?? '').trim(),
      imageMediaType: (json['imageMediaType'] as String? ?? '').trim(),
      profileFields: (json['profileFields'] as List<dynamic>? ?? [])
          .whereType<Map>()
          .map((m) => ProfileFieldKV.fromJson(m.cast<String, dynamic>()))
          .where((f) => f.name.isNotEmpty || f.value.isNotEmpty)
          .toList(),
      manuallyApprovesFollowers: (json['manuallyApprovesFollowers'] as bool?) ?? false,
      blockedDomains: (json['blockedDomains'] as List<dynamic>? ?? [])
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      blockedActors: (json['blockedActors'] as List<dynamic>? ?? [])
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      postDeliveryMode: postDeliveryMode,
      p2pCacheTtlSecs: (json['p2pCacheTtlSecs'] as num?)?.toInt(),
      previousPublicBaseUrl: (json['previousPublicBaseUrl'] as String?)?.trim(),
      previousRelayToken: (json['previousRelayToken'] as String?)?.trim(),
    );
  }

  Map<String, dynamic> toJson() => {
        'username': username,
        'domain': domain,
        'publicBaseUrl': publicBaseUrl,
        'relayWs': relayWs,
        'relayToken': relayToken,
        'bind': bind,
        'internalToken': internalToken,
        'p2pEnable': p2pEnable,
        'p2pRelayReserve': p2pRelayReserve,
        'webrtcEnable': webrtcEnable,
        'webrtcIceUrls': webrtcIceUrls,
        'webrtcIceUsername': webrtcIceUsername,
        'webrtcIceCredential': webrtcIceCredential,
        'apRelays': apRelays,
        'displayName': displayName,
        'summary': summary,
        'iconUrl': iconUrl,
        'iconMediaType': iconMediaType,
        'imageUrl': imageUrl,
        'imageMediaType': imageMediaType,
        'profileFields': profileFields.map((f) => f.toJson()).toList(),
        'manuallyApprovesFollowers': manuallyApprovesFollowers,
        'blockedDomains': blockedDomains,
        'blockedActors': blockedActors,
        'postDeliveryMode': postDeliveryMode,
        'p2pCacheTtlSecs': p2pCacheTtlSecs,
        'previousPublicBaseUrl': previousPublicBaseUrl,
        'previousRelayToken': previousRelayToken,
      };

  Map<String, dynamic> toCoreStartJson() {
    String textToHtml(String input) {
      var s = input.trim();
      if (s.isEmpty) return '';
      s = s
          .replaceAll('&', '&amp;')
          .replaceAll('<', '&lt;')
          .replaceAll('>', '&gt;')
          .replaceAll('"', '&quot;')
          .replaceAll("'", '&#39;');
      s = s.replaceAll('\r\n', '\n').replaceAll('\r', '\n').replaceAll('\n', '<br>');
      return '<p>$s</p>';
    }

    final cfg = <String, dynamic>{
      'username': username,
      'domain': domain,
      'public_base_url': publicBaseUrl,
      'relay_ws': relayWs,
      'relay_token': relayToken,
      'bind': bind,
      'internal_token': internalToken,
      'post_delivery_mode': postDeliveryMode,
      'p2p': {
        'enable': p2pEnable,
        if (p2pRelayReserve.isNotEmpty) 'relay_reserve': p2pRelayReserve,
        'webrtc_enable': webrtcEnable,
        if (webrtcIceUrls.isNotEmpty) 'webrtc_ice_urls': webrtcIceUrls,
        if (webrtcIceUsername != null && webrtcIceUsername!.trim().isNotEmpty)
          'webrtc_ice_username': webrtcIceUsername!.trim(),
        if (webrtcIceCredential != null && webrtcIceCredential!.trim().isNotEmpty)
          'webrtc_ice_credential': webrtcIceCredential!.trim(),
      },
    };
    if (displayName.trim().isNotEmpty) cfg['display_name'] = displayName.trim();
    if (summary.trim().isNotEmpty) cfg['summary'] = textToHtml(summary);
    if (iconUrl.trim().isNotEmpty) cfg['icon_url'] = iconUrl.trim();
    if (iconMediaType.trim().isNotEmpty) cfg['icon_media_type'] = iconMediaType.trim();
    if (imageUrl.trim().isNotEmpty) cfg['image_url'] = imageUrl.trim();
    if (imageMediaType.trim().isNotEmpty) cfg['image_media_type'] = imageMediaType.trim();
    if (profileFields.isNotEmpty) {
      cfg['profile_fields'] = profileFields
          .map(
            (f) => {
              'name': f.name.trim(),
              'value': textToHtml(f.value),
            },
          )
          .where((m) => (m['name'] as String).isNotEmpty && (m['value'] as String).isNotEmpty)
          .toList();
    }
    if (manuallyApprovesFollowers) {
      cfg['manually_approves_followers'] = true;
    }
    if (blockedDomains.isNotEmpty) cfg['blocked_domains'] = blockedDomains;
    if (blockedActors.isNotEmpty) cfg['blocked_actors'] = blockedActors;
    if (apRelays.isNotEmpty) {
      cfg['ap_relays'] = apRelays;
    }
    if (p2pCacheTtlSecs != null && p2pCacheTtlSecs! > 0) {
      cfg['p2p_cache_ttl_secs'] = p2pCacheTtlSecs;
    }
    if (previousPublicBaseUrl != null && previousPublicBaseUrl!.trim().isNotEmpty) {
      cfg['previous_public_base_url'] = previousPublicBaseUrl!.trim();
    }
    if (previousRelayToken != null && previousRelayToken!.trim().isNotEmpty) {
      cfg['previous_relay_token'] = previousRelayToken!.trim();
    }
    return cfg;
  }

  static String randomToken() {
    const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
    final rnd = Random.secure();
    return List.generate(32, (_) => chars[rnd.nextInt(chars.length)]).join();
  }
}
