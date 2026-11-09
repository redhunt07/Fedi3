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
        name: json['name'] is String ? (json['name'] as String).trim() : '',
        value: json['value'] is String ? (json['value'] as String).trim() : '',
      );

  Map<String, dynamic> toJson() => {'name': name, 'value': value};
}

class CoreConfig {
  CoreConfig({
    required String username,
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
    this.p2pBootstrap = const [],
    this.p2pPublicDiscovery = true,
    this.p2pPeerExchange = true,
    this.webrtcEnable = false,
    List<String> webrtcIceUrls = const [],
    this.webrtcIceUsername,
    this.webrtcIceCredential,
    this.p2pCacheTtlSecs,
    this.previousPublicBaseUrl,
    this.previousRelayToken,
    this.identityPrivateKeyPem,
    this.chatIdentityPublicB64,
    this.chatIdentitySecretB64,
    this.deviceSyncEnable = true,
    String? anchorActorUrl,
  })  : username = username.toLowerCase(),
        webrtcIceUrls = List.unmodifiable(webrtcIceUrls),
        anchorActorUrl = normalizeAnchorActorUrl(anchorActorUrl);

  final String username;
  final String domain;
  final String publicBaseUrl;
  final String relayWs;
  final String relayToken;
  final String bind;
  final String internalToken;
  final bool p2pEnable;
  final List<String> p2pRelayReserve;
  final List<String> p2pBootstrap;
  final bool p2pPublicDiscovery;
  final bool p2pPeerExchange;
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

  /// Account identity transferred to a linked client. All three are secrets
  /// and live in the OS keyring, never in the plaintext config file.
  final String? identityPrivateKeyPem;
  final String? chatIdentityPublicB64;
  final String? chatIdentitySecretB64;

  /// Replicate chat activity (messages, calls, receipts) across the account's
  /// linked devices through the relay spool.
  final bool deviceSyncEnable;

  /// Optional identity anchor: a static domain that hosts the actor document
  /// and WebFinger, decoupling the identity from the relay. Normalized to
  /// `http(s)://host` without a trailing slash, or null when unset/invalid.
  final String? anchorActorUrl;

  Uri get localBaseUri => Uri.parse('http://$bind');

  /// H-64: screens used to rebuild the whole config field by field; a single
  /// missing field silently reset it. `copyWith` keeps edits surgical.
  CoreConfig copyWith({
    String? username,
    String? domain,
    String? publicBaseUrl,
    String? relayWs,
    String? relayToken,
    String? bind,
    String? internalToken,
    bool? p2pEnable,
    List<String>? p2pRelayReserve,
    List<String>? p2pBootstrap,
    bool? p2pPublicDiscovery,
    bool? p2pPeerExchange,
    bool? webrtcEnable,
    List<String>? webrtcIceUrls,
    Object? webrtcIceUsername = _unset,
    Object? webrtcIceCredential = _unset,
    List<String>? apRelays,
    String? displayName,
    String? summary,
    String? iconUrl,
    String? iconMediaType,
    String? imageUrl,
    String? imageMediaType,
    List<ProfileFieldKV>? profileFields,
    bool? manuallyApprovesFollowers,
    List<String>? blockedDomains,
    List<String>? blockedActors,
    String? postDeliveryMode,
    Object? p2pCacheTtlSecs = _unset,
    Object? previousPublicBaseUrl = _unset,
    Object? previousRelayToken = _unset,
    Object? identityPrivateKeyPem = _unset,
    Object? chatIdentityPublicB64 = _unset,
    Object? chatIdentitySecretB64 = _unset,
    bool? deviceSyncEnable,
    Object? anchorActorUrl = _unset,
  }) {
    return CoreConfig(
      username: username ?? this.username,
      domain: domain ?? this.domain,
      publicBaseUrl: publicBaseUrl ?? this.publicBaseUrl,
      relayWs: relayWs ?? this.relayWs,
      relayToken: relayToken ?? this.relayToken,
      bind: bind ?? this.bind,
      internalToken: internalToken ?? this.internalToken,
      p2pEnable: p2pEnable ?? this.p2pEnable,
      p2pRelayReserve: p2pRelayReserve ?? this.p2pRelayReserve,
      p2pBootstrap: p2pBootstrap ?? this.p2pBootstrap,
      p2pPublicDiscovery: p2pPublicDiscovery ?? this.p2pPublicDiscovery,
      p2pPeerExchange: p2pPeerExchange ?? this.p2pPeerExchange,
      webrtcEnable: webrtcEnable ?? this.webrtcEnable,
      webrtcIceUrls: webrtcIceUrls ?? this.webrtcIceUrls,
      webrtcIceUsername: webrtcIceUsername == _unset
          ? this.webrtcIceUsername
          : webrtcIceUsername as String?,
      webrtcIceCredential: webrtcIceCredential == _unset
          ? this.webrtcIceCredential
          : webrtcIceCredential as String?,
      apRelays: apRelays ?? this.apRelays,
      displayName: displayName ?? this.displayName,
      summary: summary ?? this.summary,
      iconUrl: iconUrl ?? this.iconUrl,
      iconMediaType: iconMediaType ?? this.iconMediaType,
      imageUrl: imageUrl ?? this.imageUrl,
      imageMediaType: imageMediaType ?? this.imageMediaType,
      profileFields: profileFields ?? this.profileFields,
      manuallyApprovesFollowers:
          manuallyApprovesFollowers ?? this.manuallyApprovesFollowers,
      blockedDomains: blockedDomains ?? this.blockedDomains,
      blockedActors: blockedActors ?? this.blockedActors,
      postDeliveryMode: postDeliveryMode ?? this.postDeliveryMode,
      p2pCacheTtlSecs: p2pCacheTtlSecs == _unset
          ? this.p2pCacheTtlSecs
          : p2pCacheTtlSecs as int?,
      previousPublicBaseUrl: previousPublicBaseUrl == _unset
          ? this.previousPublicBaseUrl
          : previousPublicBaseUrl as String?,
      previousRelayToken: previousRelayToken == _unset
          ? this.previousRelayToken
          : previousRelayToken as String?,
      identityPrivateKeyPem: identityPrivateKeyPem == _unset
          ? this.identityPrivateKeyPem
          : identityPrivateKeyPem as String?,
      chatIdentityPublicB64: chatIdentityPublicB64 == _unset
          ? this.chatIdentityPublicB64
          : chatIdentityPublicB64 as String?,
      chatIdentitySecretB64: chatIdentitySecretB64 == _unset
          ? this.chatIdentitySecretB64
          : chatIdentitySecretB64 as String?,
      deviceSyncEnable: deviceSyncEnable ?? this.deviceSyncEnable,
      anchorActorUrl: anchorActorUrl == _unset
          ? this.anchorActorUrl
          : anchorActorUrl as String?,
    );
  }

  static const Object _unset = Object();

  static const String postDeliveryP2pRelay = 'p2p_relay';
  static const String postDeliveryP2pOnly = 'p2p_only';

  static String _str(dynamic value) => value is String ? value.trim() : '';

  static String? _strOrNull(dynamic value) {
    if (value is! String) return null;
    final v = value.trim();
    return v.isEmpty ? null : v;
  }

  static bool _bool(dynamic value) => value is bool ? value : false;

  static int? _intOrNull(dynamic value) => value is num ? value.toInt() : null;

  static List<String> _stringList(dynamic value) {
    if (value is! List) return const [];
    return value
        .whereType<String>()
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList();
  }

  /// Normalizes the optional identity anchor. Only `http://` and `https://`
  /// URLs are accepted; trailing slashes are stripped and anything else is
  /// treated as unset (null).
  static String? normalizeAnchorActorUrl(dynamic value) {
    if (value is! String) return null;
    var v = value.trim();
    while (v.endsWith('/')) {
      v = v.substring(0, v.length - 1);
    }
    if (!v.startsWith('http://') && !v.startsWith('https://')) return null;
    final rest = v.startsWith('https://') ? v.substring(8) : v.substring(7);
    if (rest.isEmpty) return null;
    return v;
  }

  /// AM-03/AM-04: missing or wrongly typed fields fall back to safe defaults
  /// instead of throwing a `TypeError`.
  static CoreConfig fromJson(Map<String, dynamic> json) {
    final rawMode = _str(json['postDeliveryMode']).toLowerCase();
    final postDeliveryMode =
        (rawMode == postDeliveryP2pOnly || rawMode == postDeliveryP2pRelay)
            ? rawMode
            : postDeliveryP2pRelay;
    final p2pEnable = _bool(json['p2pEnable']);
    return CoreConfig(
      username: _str(json['username']),
      domain: _str(json['domain']),
      publicBaseUrl: _str(json['publicBaseUrl']),
      relayWs: _str(json['relayWs']),
      relayToken: _str(json['relayToken']),
      bind: _str(json['bind']),
      internalToken: _str(json['internalToken']),
      p2pEnable: p2pEnable,
      p2pRelayReserve: _stringList(json['p2pRelayReserve']),
      p2pBootstrap: _stringList(json['p2pBootstrap']),
      p2pPublicDiscovery: json['p2pPublicDiscovery'] == null
          ? true
          : _bool(json['p2pPublicDiscovery']),
      p2pPeerExchange: json['p2pPeerExchange'] == null
          ? true
          : _bool(json['p2pPeerExchange']),
      webrtcEnable: json['webrtcEnable'] == null
          ? p2pEnable
          : _bool(json['webrtcEnable']),
      webrtcIceUrls: _stringList(json['webrtcIceUrls']),
      webrtcIceUsername: _strOrNull(json['webrtcIceUsername']),
      webrtcIceCredential: _strOrNull(json['webrtcIceCredential']),
      apRelays: _stringList(json['apRelays']),
      displayName: _str(json['displayName']),
      summary: _str(json['summary']),
      iconUrl: _str(json['iconUrl']),
      iconMediaType: _str(json['iconMediaType']),
      imageUrl: _str(json['imageUrl']),
      imageMediaType: _str(json['imageMediaType']),
      profileFields: json['profileFields'] is List
          ? (json['profileFields'] as List)
              .whereType<Map>()
              .map((m) => ProfileFieldKV.fromJson(m.cast<String, dynamic>()))
              .where((f) => f.name.isNotEmpty || f.value.isNotEmpty)
              .toList()
          : const [],
      manuallyApprovesFollowers: _bool(json['manuallyApprovesFollowers']),
      blockedDomains: _stringList(json['blockedDomains']),
      blockedActors: _stringList(json['blockedActors']),
      postDeliveryMode: postDeliveryMode,
      p2pCacheTtlSecs: _intOrNull(json['p2pCacheTtlSecs']),
      previousPublicBaseUrl: _strOrNull(json['previousPublicBaseUrl']),
      previousRelayToken: _strOrNull(json['previousRelayToken']),
      identityPrivateKeyPem: _strOrNull(json['identityPrivateKeyPem']),
      chatIdentityPublicB64: _strOrNull(json['chatIdentityPublicB64']),
      chatIdentitySecretB64: _strOrNull(json['chatIdentitySecretB64']),
      deviceSyncEnable: json['deviceSyncEnable'] == null
          ? true
          : _bool(json['deviceSyncEnable']),
      anchorActorUrl: normalizeAnchorActorUrl(json['anchorActorUrl']),
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
        'p2pBootstrap': p2pBootstrap,
        'p2pPublicDiscovery': p2pPublicDiscovery,
        'p2pPeerExchange': p2pPeerExchange,
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
        'identityPrivateKeyPem': identityPrivateKeyPem,
        'chatIdentityPublicB64': chatIdentityPublicB64,
        'chatIdentitySecretB64': chatIdentitySecretB64,
        'deviceSyncEnable': deviceSyncEnable,
        'anchorActorUrl': anchorActorUrl,
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
      s = s
          .replaceAll('\r\n', '\n')
          .replaceAll('\r', '\n')
          .replaceAll('\n', '<br>');
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
        'bootstrap': p2pBootstrap,
        'public_discovery_enable': p2pPublicDiscovery,
        'peer_exchange_enable': p2pPeerExchange,
        if (p2pRelayReserve.isNotEmpty) 'relay_reserve': p2pRelayReserve,
        'webrtc_enable': webrtcEnable,
        // webrtc-rs currently supports UDP TURN only. Preserve TCP/TLS URLs
        // in app settings for the native audio/video calling implementation.
        if (webrtcIceUrls.isNotEmpty)
          'webrtc_ice_urls': webrtcIceUrls.where((url) {
            final lower = url.toLowerCase();
            return !lower.contains('transport=tcp') &&
                !lower.startsWith('turns:');
          }).toList(growable: false),
        if (webrtcIceUsername != null && webrtcIceUsername!.trim().isNotEmpty)
          'webrtc_ice_username': webrtcIceUsername!.trim(),
        if (webrtcIceCredential != null &&
            webrtcIceCredential!.trim().isNotEmpty)
          'webrtc_ice_credential': webrtcIceCredential!.trim(),
        'device_sync_enable': deviceSyncEnable,
        'device_sync_poll_secs': 3,
      },
    };
    if (identityPrivateKeyPem != null &&
        identityPrivateKeyPem!.trim().isNotEmpty) {
      cfg['identity_private_key_pem'] = identityPrivateKeyPem!.trim();
    }
    if (chatIdentityPublicB64 != null &&
        chatIdentityPublicB64!.trim().isNotEmpty) {
      cfg['chat_identity_public_b64'] = chatIdentityPublicB64!.trim();
    }
    if (chatIdentitySecretB64 != null &&
        chatIdentitySecretB64!.trim().isNotEmpty) {
      cfg['chat_identity_secret_b64'] = chatIdentitySecretB64!.trim();
    }
    if (displayName.trim().isNotEmpty) cfg['display_name'] = displayName.trim();
    if (summary.trim().isNotEmpty) cfg['summary'] = textToHtml(summary);
    if (iconUrl.trim().isNotEmpty) cfg['icon_url'] = iconUrl.trim();
    if (iconMediaType.trim().isNotEmpty) {
      cfg['icon_media_type'] = iconMediaType.trim();
    }
    if (imageUrl.trim().isNotEmpty) cfg['image_url'] = imageUrl.trim();
    if (imageMediaType.trim().isNotEmpty) {
      cfg['image_media_type'] = imageMediaType.trim();
    }
    if (profileFields.isNotEmpty) {
      cfg['profile_fields'] = profileFields
          .map(
            (f) => {
              'name': f.name.trim(),
              'value': textToHtml(f.value),
            },
          )
          .where((m) =>
              (m['name'] as String).isNotEmpty &&
              (m['value'] as String).isNotEmpty)
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
    if (previousPublicBaseUrl != null &&
        previousPublicBaseUrl!.trim().isNotEmpty) {
      cfg['previous_public_base_url'] = previousPublicBaseUrl!.trim();
    }
    if (previousRelayToken != null && previousRelayToken!.trim().isNotEmpty) {
      cfg['previous_relay_token'] = previousRelayToken!.trim();
    }
    if (anchorActorUrl != null && anchorActorUrl!.isNotEmpty) {
      cfg['anchor_actor_url'] = anchorActorUrl;
    }
    return cfg;
  }

  static String randomToken() {
    const chars =
        'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
    final rnd = Random.secure();
    return List.generate(32, (_) => chars[rnd.nextInt(chars.length)]).join();
  }
}
