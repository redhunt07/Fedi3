/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:convert';

class ChatThreadItem {
  ChatThreadItem({
    required this.threadId,
    required this.kind,
    required this.title,
    required this.createdAtMs,
    required this.updatedAtMs,
    required this.lastMessageMs,
    required this.lastMessagePreview,
  });

  final String threadId;
  final String kind;
  final String? title;
  final int createdAtMs;
  final int updatedAtMs;
  final int? lastMessageMs;
  final String? lastMessagePreview;

  factory ChatThreadItem.fromJson(Map<String, dynamic> json) {
    return ChatThreadItem(
      threadId: json['thread_id']?.toString() ?? '',
      kind: json['kind']?.toString() ?? 'group',
      title: json['title']?.toString(),
      createdAtMs: (json['created_at_ms'] as num?)?.toInt() ?? 0,
      updatedAtMs: (json['updated_at_ms'] as num?)?.toInt() ?? 0,
      lastMessageMs: (json['last_message_ms'] as num?)?.toInt(),
      lastMessagePreview: json['last_message_preview']?.toString(),
    );
  }
}

class ChatMessageItem {
  ChatMessageItem({
    required this.messageId,
    required this.threadId,
    required this.senderActor,
    required this.senderDevice,
    required this.createdAtMs,
    required this.editedAtMs,
    required this.deleted,
    required this.bodyJson,
  });

  final String messageId;
  final String threadId;
  final String senderActor;
  final String senderDevice;
  final int createdAtMs;
  final int? editedAtMs;
  final bool deleted;
  final String bodyJson;

  factory ChatMessageItem.fromJson(Map<String, dynamic> json) {
    return ChatMessageItem(
      messageId: json['message_id']?.toString() ?? '',
      threadId: json['thread_id']?.toString() ?? '',
      senderActor: json['sender_actor']?.toString() ?? '',
      senderDevice: json['sender_device']?.toString() ?? '',
      createdAtMs: (json['created_at_ms'] as num?)?.toInt() ?? 0,
      editedAtMs: (json['edited_at_ms'] as num?)?.toInt(),
      deleted: (json['deleted'] as bool?) ?? ((json['deleted'] as num?)?.toInt() ?? 0) != 0,
      bodyJson: json['body_json']?.toString() ?? '{}',
    );
  }

  ChatPayload? get payload {
    try {
      final raw = jsonDecode(bodyJson);
      if (raw is Map<String, dynamic>) {
        return ChatPayload.fromJson(raw);
      }
    } catch (_) {}
    return null;
  }
}

class ChatPayload {
  ChatPayload({
    required this.op,
    this.text,
    this.replyTo,
    this.messageId,
    this.status,
    this.threadId,
    this.attachments,
    this.action,
    this.targets,
    this.members,
    this.title,
    this.reaction,
  });

  final String op;
  final String? text;
  final String? replyTo;
  final String? messageId;
  final String? status;
  final String? threadId;
  final List<ChatAttachment>? attachments;
  final String? action;
  final List<String>? targets;
  final List<String>? members;
  final String? title;
  final String? reaction;

  factory ChatPayload.fromJson(Map<String, dynamic> json) {
    return ChatPayload(
      op: json['op']?.toString() ?? '',
      text: json['text']?.toString(),
      replyTo: json['reply_to']?.toString(),
      messageId: json['message_id']?.toString(),
      status: json['status']?.toString(),
      threadId: json['thread_id']?.toString(),
      attachments: (json['attachments'] is List)
          ? (json['attachments'] as List)
              .whereType<Map>()
              .map((m) => ChatAttachment.fromJson(m.cast<String, dynamic>()))
              .toList()
          : null,
      action: json['action']?.toString(),
      targets: (json['targets'] is List)
          ? (json['targets'] as List).map((v) => v.toString()).toList()
          : null,
      members: (json['members'] is List)
          ? (json['members'] as List).map((v) => v.toString()).toList()
          : null,
      title: json['title']?.toString(),
      reaction: json['reaction']?.toString(),
    );
  }
}

class ChatAttachment {
  ChatAttachment({
    required this.id,
    required this.url,
    required this.mediaType,
    this.name,
    this.width,
    this.height,
    this.blurhash,
  });

  final String id;
  final String url;
  final String mediaType;
  final String? name;
  final int? width;
  final int? height;
  final String? blurhash;

  factory ChatAttachment.fromJson(Map<String, dynamic> json) {
    return ChatAttachment(
      id: json['id']?.toString() ?? '',
      url: json['url']?.toString() ?? '',
      mediaType: json['mediaType']?.toString() ?? json['media_type']?.toString() ?? '',
      name: json['name']?.toString(),
      width: (json['width'] as num?)?.toInt(),
      height: (json['height'] as num?)?.toInt(),
      blurhash: json['blurhash']?.toString(),
    );
  }
}
