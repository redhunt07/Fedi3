/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:xml/xml.dart';

import 'rss_feed_store.dart';

class RssItem {
  const RssItem({
    required this.title,
    required this.link,
    required this.publishedMs,
    required this.source,
  });

  final String title;
  final String link;
  final int publishedMs;
  final String source;
}

class RssFeedService {
  RssFeedService._();

  static final RssFeedService instance = RssFeedService._();

  final ValueNotifier<List<String>> urls =
      ValueNotifier<List<String>>(const []);
  final ValueNotifier<List<RssItem>> items =
      ValueNotifier<List<RssItem>>(const []);

  Timer? _poll;
  bool _loading = false;

  Future<void> start() async {
    urls.value = await RssFeedStore.instance.readUrls();
    await refreshNow();
    _poll?.cancel();
    _poll = Timer.periodic(const Duration(minutes: 10), (_) {
      unawaited(refreshNow());
    });
  }

  void stop() {
    _poll?.cancel();
    _poll = null;
  }

  Future<void> saveUrls(List<String> next) async {
    await RssFeedStore.instance.writeUrls(next);
    urls.value = await RssFeedStore.instance.readUrls();
    await refreshNow();
  }

  Future<void> refreshNow() async {
    if (_loading) return;
    _loading = true;
    try {
      final current = urls.value;
      if (current.isEmpty) {
        items.value = const [];
        return;
      }
      final nextItems = <RssItem>[];
      for (final url in current) {
        final parsed = await _fetchFeed(url);
        nextItems.addAll(parsed);
      }
      nextItems.sort((a, b) => b.publishedMs.compareTo(a.publishedMs));
      final dedup = <String, RssItem>{};
      for (final item in nextItems) {
        final key = '${item.link}|${item.title}';
        dedup.putIfAbsent(key, () => item);
      }
      items.value = dedup.values.take(80).toList(growable: false);
    } catch (_) {
      // keep previous snapshot on parser/fetch errors
    } finally {
      _loading = false;
    }
  }

  Future<List<RssItem>> _fetchFeed(String url) async {
    try {
      final uri = Uri.tryParse(url);
      if (uri == null) return const [];
      final resp = await http.get(uri, headers: const {
        'Accept':
            'application/rss+xml, application/atom+xml, application/xml, text/xml'
      }).timeout(const Duration(seconds: 8));
      if (resp.statusCode < 200 || resp.statusCode >= 300) return const [];
      return _parseFeed(resp.body, url);
    } catch (_) {
      return const [];
    }
  }

  List<RssItem> _parseFeed(String body, String source) {
    late final XmlDocument doc;
    try {
      doc = XmlDocument.parse(body);
    } catch (_) {
      return const [];
    }
    final out = <RssItem>[];

    final rssItems = doc.findAllElements('item');
    for (final node in rssItems) {
      final title = _text(node, 'title');
      final link = _text(node, 'link');
      if (title.isEmpty || link.isEmpty) continue;
      out.add(RssItem(
        title: title,
        link: link,
        publishedMs: _parseDate(_text(node, 'pubDate')),
        source: source,
      ));
    }

    final atomEntries = doc.findAllElements('entry');
    for (final node in atomEntries) {
      final title = _text(node, 'title');
      final linkNode = node.findElements('link').firstWhere(
            (e) =>
                (e.getAttribute('rel') ?? '').isEmpty ||
                e.getAttribute('rel') == 'alternate',
            orElse: () => XmlElement(XmlName('link')),
          );
      final link = (linkNode.getAttribute('href') ?? '').trim();
      if (title.isEmpty || link.isEmpty) continue;
      out.add(RssItem(
        title: title,
        link: link,
        publishedMs: _parseDate(_text(node, 'updated').isNotEmpty
            ? _text(node, 'updated')
            : _text(node, 'published')),
        source: source,
      ));
    }
    return out;
  }

  String _text(XmlElement parent, String name) {
    return parent.findElements(name).map((e) => e.innerText.trim()).firstWhere(
          (v) => v.isNotEmpty,
          orElse: () => '',
        );
  }

  int _parseDate(String input) {
    final dt = DateTime.tryParse(input.trim());
    if (dt == null) return 0;
    return dt.toUtc().millisecondsSinceEpoch;
  }
}
