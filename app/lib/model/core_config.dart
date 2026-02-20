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
    required this.apRelays,
    required this.bootstrapFollowActors,
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
    this.previousPublicBaseUrl,
    this.previousRelayToken,
    this.upnpPortRangeStart,
    this.upnpPortRangeEnd,
    this.upnpLeaseSecs,
    this.upnpTimeoutSecs,
    this.useTor = false,
    this.proxyHost,
    this.proxyPort,
    this.proxyType,
  });

  final String username;
  final String domain;
  final String publicBaseUrl;
  final String relayWs;
  final String relayToken;
  final String bind;
  final String internalToken;
  final List<String> apRelays;
  final List<String> bootstrapFollowActors;
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
  final String? previousPublicBaseUrl;
  final String? previousRelayToken;
  final int? upnpPortRangeStart;
  final int? upnpPortRangeEnd;
  final int? upnpLeaseSecs;
  final int? upnpTimeoutSecs;
  final bool useTor;
  final String? proxyHost;
  final int? proxyPort;
  final String? proxyType;

  Uri get localBaseUri => Uri.parse('http://$bind');

  static CoreConfig fromJson(Map<String, dynamic> json) {
    return CoreConfig(
      username: (json['username'] as String? ?? '').trim(),
      domain: (json['domain'] as String? ?? '').trim(),
      publicBaseUrl: (json['publicBaseUrl'] as String? ?? '').trim(),
      relayWs: (json['relayWs'] as String? ?? '').trim(),
      relayToken: (json['relayToken'] as String? ?? '').trim(),
      bind: (json['bind'] as String? ?? '').trim(),
      internalToken: (json['internalToken'] as String? ?? '').trim(),
      apRelays: (json['apRelays'] as List<dynamic>? ?? [])
          .whereType<String>()
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList(),
      bootstrapFollowActors: (json['bootstrapFollowActors'] as List<dynamic>? ?? [])
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
      previousPublicBaseUrl: (json['previousPublicBaseUrl'] as String?)?.trim(),
      previousRelayToken: (json['previousRelayToken'] as String?)?.trim(),
      upnpPortRangeStart: (json['upnpPortRangeStart'] as num?)?.toInt(),
      upnpPortRangeEnd: (json['upnpPortRangeEnd'] as num?)?.toInt(),
      upnpLeaseSecs: (json['upnpLeaseSecs'] as num?)?.toInt(),
      upnpTimeoutSecs: (json['upnpTimeoutSecs'] as num?)?.toInt(),
      useTor: (json['useTor'] as bool?) ?? false,
      proxyHost: (json['proxyHost'] as String?)?.trim(),
      proxyPort: (json['proxyPort'] as num?)?.toInt(),
      proxyType: (json['proxyType'] as String?)?.trim(),
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
        'apRelays': apRelays,
        'bootstrapFollowActors': bootstrapFollowActors,
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
        'previousPublicBaseUrl': previousPublicBaseUrl,
        'previousRelayToken': previousRelayToken,
        'upnpPortRangeStart': upnpPortRangeStart,
        'upnpPortRangeEnd': upnpPortRangeEnd,
        'upnpLeaseSecs': upnpLeaseSecs,
        'upnpTimeoutSecs': upnpTimeoutSecs,
        'useTor': useTor,
        'proxyHost': proxyHost,
        'proxyPort': proxyPort,
        'proxyType': proxyType,
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
    if (bootstrapFollowActors.isNotEmpty) {
      cfg['bootstrap_follow_actors'] = bootstrapFollowActors;
    }
    if (previousPublicBaseUrl != null && previousPublicBaseUrl!.trim().isNotEmpty) {
      cfg['previous_public_base_url'] = previousPublicBaseUrl!.trim();
    }
    if (previousRelayToken != null && previousRelayToken!.trim().isNotEmpty) {
      cfg['previous_relay_token'] = previousRelayToken!.trim();
    }
    if (upnpPortRangeStart != null &&
        upnpPortRangeEnd != null &&
        upnpPortRangeStart! > 0 &&
        upnpPortRangeStart! <= upnpPortRangeEnd!) {
      cfg['upnp_port_start'] = upnpPortRangeStart;
      cfg['upnp_port_end'] = upnpPortRangeEnd;
      if (upnpLeaseSecs != null && upnpLeaseSecs! > 0) {
        cfg['upnp_lease_secs'] = upnpLeaseSecs;
      }
      if (upnpTimeoutSecs != null && upnpTimeoutSecs! > 0) {
        cfg['upnp_timeout_secs'] = upnpTimeoutSecs;
      }
    }
    return cfg;
  }

  static String randomToken() {
    const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
    final rnd = Random.secure();
    return List.generate(32, (_) => chars[rnd.nextInt(chars.length)]).join();
  }
}
