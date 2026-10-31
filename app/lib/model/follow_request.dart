/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

/// A pending follow request received by a locked account (H-44).
class FollowRequest {
  const FollowRequest({
    required this.actor,
    required this.createdAtMs,
    this.state = 'pending',
  });

  final String actor;
  final int createdAtMs;
  final String state;

  factory FollowRequest.fromJson(Map<String, dynamic> json) {
    final actor = (json['actor'] as String? ?? '').trim();
    final created = json['created_at_ms'];
    final createdAtMs =
        created is int ? created : int.tryParse('${created ?? ''}') ?? 0;
    return FollowRequest(
      actor: actor,
      createdAtMs: createdAtMs,
      state: (json['state'] as String? ?? 'pending'),
    );
  }

  DateTime? get createdAt => createdAtMs <= 0
      ? null
      : DateTime.fromMillisecondsSinceEpoch(createdAtMs);

  /// Short display handle: `user@host` when the actor is a well-formed URL.
  String get handle {
    final uri = Uri.tryParse(actor);
    if (uri == null || uri.host.isEmpty) return actor;
    final segments = uri.pathSegments;
    final user = segments.isNotEmpty ? segments.last : '';
    return user.isEmpty ? uri.host : '$user@${uri.host}';
  }
}
