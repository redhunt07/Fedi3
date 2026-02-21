/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import '../utils/mfm_codec.dart';

class NoteEmoji {
  NoteEmoji({required this.name, required this.iconUrl});

  final String name;
  final String iconUrl;

  static List<NoteEmoji> parseList(dynamic tags) {
    if (tags is! List) return const [];
    final out = <NoteEmoji>[];
    for (final t in tags) {
      if (t is! Map) continue;
      final m = t.cast<String, dynamic>();
      if ((m['type'] as String?) != 'Emoji') continue;
      final name = (m['name'] as String?)?.trim() ?? '';
      if (name.isEmpty) continue;
      final icon = m['icon'];
      String url = '';
      if (icon is Map) {
        final u = icon['url'];
        if (u is String) url = u.trim();
        if (u is Map) url = (u['href'] as String?)?.trim() ?? '';
      }
      if (url.isEmpty) continue;
      out.add(NoteEmoji(name: name, iconUrl: url));
    }
    return out;
  }
}

class NoteAttachment {
  NoteAttachment({required this.url, required this.mediaType});

  final String url;
  final String mediaType;

  static List<NoteAttachment> parseList(dynamic raw) {
    if (raw is Map) {
      raw = [raw];
    }
    if (raw is! List) return const [];
    final out = <NoteAttachment>[];
    for (final it in raw) {
      if (it is String) {
        final s = it.trim();
        if (s.isNotEmpty) out.add(NoteAttachment(url: s, mediaType: ''));
        continue;
      }
      if (it is! Map) continue;
      final m = it.cast<String, dynamic>();
      var url = '';
      final u = m['url'];
      if (u is String) url = u.trim();
      if (u is Map) url = (u['href'] as String?)?.trim() ?? '';
      if (u is List) {
        for (final v in u) {
          if (v is String && v.trim().isNotEmpty) {
            url = v.trim();
            break;
          }
          if (v is Map) {
            final href = (v['href'] as String?)?.trim() ?? '';
            if (href.isNotEmpty) {
              url = href;
              break;
            }
          }
        }
      }
      if (url.isEmpty) {
        final href = (m['href'] as String?)?.trim() ?? '';
        if (href.isNotEmpty) url = href;
      }
      final mt = (m['mediaType'] as String?)?.trim() ?? '';
      if (url.isNotEmpty) out.add(NoteAttachment(url: url, mediaType: mt));
    }
    return out;
  }
}

class Note {
  Note({
    required this.id,
    required this.attributedTo,
    required this.contentHtml,
    required this.summary,
    required this.sensitive,
    required this.published,
    this.createdAtMs = 0,
    required this.inReplyTo,
    required this.attachments,
    required this.emojis,
    required this.hashtags,
  });

  final String id;
  final String attributedTo;
  final String contentHtml;
  final String summary;
  final bool sensitive;
  final String published;
  final int createdAtMs;
  final String inReplyTo;
  final List<NoteAttachment> attachments;
  final List<NoteEmoji> emojis;
  final List<String> hashtags;

  static List<String> parseHashtags(dynamic tags) {
    if (tags == null) return const [];
    final out = <String>[];
    void addTag(String value) {
      var tag = value.trim();
      if (tag.isEmpty) return;
      tag = tag.startsWith('#') ? tag.substring(1) : tag;
      if (tag.isEmpty) return;
      if (!out.contains(tag)) out.add(tag);
    }

    void parseTag(dynamic v) {
      if (v is String) {
        if (v.trim().startsWith('#')) addTag(v);
        return;
      }
      if (v is! Map) return;
      final m = v.cast<String, dynamic>();
      final type = (m['type'] as String?)?.trim() ?? '';
      final name = (m['name'] as String?)?.trim() ?? '';
      if (type == 'Hashtag' || name.startsWith('#')) {
        addTag(name);
      }
    }

    if (tags is List) {
      for (final t in tags) {
        parseTag(t);
      }
    } else {
      parseTag(tags);
    }
    return out;
  }

  static Note? tryParse(Map<String, dynamic> json) {
    final type = (json['type'] as String?)?.trim() ?? '';
    if (type != 'Note' && type != 'Article' && type != 'Question') return null;
    final id = (json['id'] as String?)?.trim() ?? '';
    if (id.isEmpty) return null;
    final attributedTo = (json['attributedTo'] as String?)?.trim() ?? '';
    var content = (json['content'] as String?)?.trim() ?? (json['name'] as String?)?.trim() ?? '';
    if (content.isEmpty) {
      content = _contentFromMap(json['contentMap']);
    }
    if (content.isEmpty) {
      final src = json['source'];
      if (src is Map) {
        final srcMap = src.cast<String, dynamic>();
        final srcContent = (srcMap['content'] as String?)?.trim() ?? '';
        if (srcContent.isNotEmpty) {
          final mediaType = (srcMap['mediaType'] as String?)?.toLowerCase().trim() ?? '';
          if (mediaType.contains('text/plain') && !_looksLikeHtml(srcContent)) {
            content = _htmlFromPlain(srcContent);
          } else {
            content = srcContent;
          }
        }
      }
    }
    if (content.isNotEmpty && !_looksLikeHtml(content)) {
      if (MfmCodec.hasMarkers(content)) {
        content = MfmCodec.toHtml(content);
      } else {
        content = _htmlFromPlain(content);
      }
    }
    final contentHtml = _normalizeHtmlEntities(content);
    var summary = (json['summary'] as String?)?.trim() ?? '';
    if (summary.isEmpty) {
      summary = _contentFromMap(json['summaryMap']);
    }
    if (summary.isNotEmpty && !_looksLikeHtml(summary)) {
      if (MfmCodec.hasMarkers(summary)) {
        summary = MfmCodec.toHtml(summary);
      } else {
        summary = _htmlFromPlain(summary);
      }
    }
    final summaryHtml = _normalizeHtmlEntities(summary);
    final sensitive = (json['sensitive'] as bool?) ?? summary.isNotEmpty;
    final published = (json['published'] as String?)?.trim() ?? '';
    final inReplyTo = (json['inReplyTo'] as String?)?.trim() ?? '';
    final createdAtMs = _readCreatedAtMs(json);
    final attachments = NoteAttachment.parseList(json['attachment']);
    final emojis = NoteEmoji.parseList(json['tag']);
    final hashtags = parseHashtags(json['tag']);
    return Note(
      id: id,
      attributedTo: attributedTo,
      contentHtml: contentHtml,
      summary: summaryHtml,
      sensitive: sensitive,
      published: published,
      createdAtMs: createdAtMs,
      inReplyTo: inReplyTo,
      attachments: attachments,
      emojis: emojis,
      hashtags: hashtags,
    );
  }
}

int _readCreatedAtMs(Map<String, dynamic> json) {
  final raw = json['created_at_ms'] ?? json['createdAtMs'];
  if (raw is num) return raw.toInt();
  if (raw is String) return int.tryParse(raw.trim()) ?? 0;
  return 0;
}

String _normalizeHtmlEntities(String input) {
  var out = input;
  if (!out.contains('&')) return out;
  out = out.replaceAll('&amp;#', '&#');
  out = out.replaceAll('&amp;quot;', '&quot;');
  out = out.replaceAll('&amp;apos;', '&apos;');
  out = out.replaceAll('&#39;', '\'');
  out = out.replaceAll('&#039;', '\'');
  out = out.replaceAll('&apos;', '\'');
  out = out.replaceAll('&#34;', '"');
  out = out.replaceAll('&quot;', '"');
  return out;
}

bool _looksLikeHtml(String input) {
  final v = input.trim();
  if (v.isEmpty) return false;
  return v.contains('<') && v.contains('>');
}

String _htmlFromPlain(String input) {
  final b = StringBuffer();
  for (final ch in input.split('')) {
    switch (ch) {
      case '&':
        b.write('&amp;');
        break;
      case '<':
        b.write('&lt;');
        break;
      case '>':
        b.write('&gt;');
        break;
      case '"':
        b.write('&quot;');
        break;
      case '\'':
        b.write('&#39;');
        break;
      case '\n':
        b.write('<br>');
        break;
      default:
        b.write(ch);
    }
  }
  return '<p>${b.toString()}</p>';
}

String _contentFromMap(dynamic map) {
  if (map is! Map) return '';
  final m = map.cast<String, dynamic>();
  const prefs = ['en', 'it', 'und'];
  for (final k in prefs) {
    final v = m[k];
    if (v is String && v.trim().isNotEmpty) return v.trim();
  }
  for (final entry in m.entries) {
    if (entry.value is String && (entry.value as String).trim().isNotEmpty) {
      return (entry.value as String).trim();
    }
  }
  return '';
}

class TimelineItem {
  TimelineItem({
    required this.activityId,
    required this.activityType,
    required this.actor,
    required this.note,
    required this.boostedBy,
    required this.inReplyToPreview,
    required this.quotePreview,
  });

  final String activityId;
  final String activityType;
  final String actor;
  final Note note;
  final String boostedBy;
  final Map<String, dynamic>? inReplyToPreview;
  final Map<String, dynamic>? quotePreview;

  static TimelineItem? tryFromActivity(Map<String, dynamic> activity) {
    final activityId = (activity['id'] as String?)?.trim() ?? '';
    final activityType = (activity['type'] as String?)?.trim() ?? '';
    final actor = (activity['actor'] as String?)?.trim() ?? '';

    final rawObj = activity['object'];
    Map<String, dynamic>? obj;
    if (rawObj is Map) obj = rawObj.cast<String, dynamic>();
    if (rawObj is String) return null;

    Map<String, dynamic>? noteJson;
    var effectiveType = activityType;
    if (activityType == 'Create') {
      final inner = obj?['object'];
      if (inner is Map) {
        noteJson = inner.cast<String, dynamic>();
      } else if (_isNoteLikeType(obj?['type'])) {
        noteJson = obj;
      }
    } else if (activityType == 'Update') {
      if (_isNoteLikeType(obj?['type'])) {
        noteJson = obj;
      } else if (obj?['object'] is Map) {
        noteJson = (obj!['object'] as Map).cast<String, dynamic>();
      }
    } else if (activityType == 'Announce') {
      if (_isNoteLikeType(obj?['type'])) {
        noteJson = obj;
      } else if (obj?['object'] is Map) {
        noteJson = (obj!['object'] as Map).cast<String, dynamic>();
      } else {
        return null;
      }
    } else if (activityType == 'Note' || activityType == 'Article' || activityType == 'Question') {
      noteJson = activity;
      effectiveType = 'Create';
    } else if (activityType.isEmpty && activity['content'] != null) {
      noteJson = activity;
      effectiveType = 'Create';
    } else {
      return null;
    }

    final note = noteJson == null ? null : Note.tryParse(noteJson);
    if (note == null) return null;

    final boostedBy = effectiveType == 'Announce' ? actor : '';
    final effectiveActor = actor.isNotEmpty
        ? actor
        : (noteJson?['attributedTo'] as String?)?.trim() ?? '';
    final inReplyToPreview = activity['fedi3InReplyToObject'] is Map ? (activity['fedi3InReplyToObject'] as Map).cast<String, dynamic>() : null;
    final quotePreview = activity['fedi3QuoteObject'] is Map ? (activity['fedi3QuoteObject'] as Map).cast<String, dynamic>() : null;

    return TimelineItem(
      activityId: activityId.isNotEmpty ? activityId : (noteJson?['id'] as String?)?.trim() ?? '',
      activityType: effectiveType.isNotEmpty ? effectiveType : activityType,
      actor: effectiveActor,
      note: note,
      boostedBy: boostedBy,
      inReplyToPreview: inReplyToPreview,
      quotePreview: quotePreview,
    );
  }
}

bool _isNoteLikeType(dynamic value) {
  final ty = value is String ? value.trim() : '';
  return ty == 'Note' || ty == 'Article' || ty == 'Question';
}
