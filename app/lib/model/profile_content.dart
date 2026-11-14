import 'package:html/dom.dart' as dom;
import 'package:html/parser.dart' as html;

/// Canonical profile HTML. Older clients escaped synchronized HTML as text on
/// each restart; unwrap only complete paragraph wrappers, never arbitrary text.
class ProfileContent {
  static final _markup = RegExp(r'</?[a-z][^>]*>', caseSensitive: false);
  static final _paragraph =
      RegExp(r'^<(p|div)(\s[^>]*)?>[\s\S]*</(p|div)>$', caseSensitive: false);
  static final _url = RegExp(r'''https?://[^\s<>"']+''');
  static const _allowed = {
    'p',
    'div',
    'span',
    'br',
    'a',
    'b',
    'strong',
    'i',
    'em',
    'u',
    's',
    'del',
    'blockquote',
    'code',
    'pre',
    'ul',
    'ol',
    'li',
    'h1',
    'h2',
    'h3'
  };
  static const _discard = {'script', 'style', 'iframe', 'object', 'embed'};

  static String toHtml(String input) {
    var raw = input.trim();
    if (raw.isEmpty) return '';
    for (var round = 0; round < 12; round++) {
      final fragment = html.parseFragment(raw);
      final nodes = fragment.nodes
          .where((n) => n is! dom.Text || n.text.trim().isNotEmpty)
          .toList();
      String? candidate;
      if (nodes.length == 1 && nodes.single is dom.Element) {
        final element = nodes.single as dom.Element;
        if ({'p', 'div'}.contains(element.localName) &&
            element.nodes.every((n) => n is dom.Text)) {
          candidate = element.text.trim();
        }
      } else if (nodes.every((n) => n is dom.Text)) {
        candidate = fragment.text?.trim();
      }
      if (candidate == null) break;
      var decoded = candidate;
      for (var depth = 0;
          depth < 12 && !_paragraph.hasMatch(decoded);
          depth++) {
        final next = html.parseFragment(decoded).text ?? '';
        if (next == decoded) break;
        decoded = next;
      }
      if (!_paragraph.hasMatch(decoded) || decoded == raw) break;
      raw = decoded;
    }
    final fragment = _markup.hasMatch(raw)
        ? html.parseFragment(raw)
        : (dom.DocumentFragment()
          ..nodes.add(dom.Element.tag('p')
            ..nodes.add(dom.Text(
                raw.replaceAll('\r\n', '\n').replaceAll('\r', '\n')))));
    final result = dom.DocumentFragment();
    for (final node in fragment.nodes.toList()) {
      result.nodes.addAll(_clean(node));
    }
    return result.outerHtml;
  }

  static Iterable<dom.Node> _clean(dom.Node node,
      {bool inLink = false, bool literal = false}) sync* {
    if (node is dom.Text) {
      if (inLink || literal) {
        yield dom.Text(node.text);
        return;
      }
      var start = 0;
      for (final match in _url.allMatches(node.text)) {
        if (match.start > start) {
          yield* _text(node.text.substring(start, match.start));
        }
        final value = match.group(0)!.replaceFirst(RegExp(r'[.,;!]+$'), '');
        yield dom.Element.tag('a')
          ..attributes['href'] = value
          ..nodes.add(dom.Text(value));
        yield* _text(match.group(0)!.substring(value.length));
        start = match.end;
      }
      if (start < node.text.length) yield* _text(node.text.substring(start));
      return;
    }
    if (node is! dom.Element || _discard.contains(node.localName)) return;
    if (node.localName == 'img') {
      yield dom.Text(node.attributes['alt'] ?? '');
      return;
    }
    final children = <dom.Node>[];
    for (final child in node.nodes) {
      children.addAll(_clean(child,
          inLink: inLink || node.localName == 'a',
          literal: literal || {'code', 'pre'}.contains(node.localName)));
    }
    if (!_allowed.contains(node.localName)) {
      yield* children;
      return;
    }
    final element = dom.Element.tag(node.localName!);
    if (node.localName == 'a') {
      final href = node.attributes['href']?.trim() ?? '';
      final uri = Uri.tryParse(href);
      if (uri != null &&
          {'http', 'https', 'mailto'}.contains(uri.scheme.toLowerCase())) {
        element.attributes['href'] = href;
      }
      if ((node.attributes['rel'] ?? '').split(' ').contains('me')) {
        element.attributes['rel'] = 'me';
      }
    }
    element.nodes.addAll(children);
    yield element;
  }

  static Iterable<dom.Node> _text(String value) sync* {
    final lines = value.split('\n');
    for (var i = 0; i < lines.length; i++) {
      if (i > 0) yield dom.Element.tag('br');
      yield dom.Text(lines[i]);
    }
  }

  static String toPlainText(String input) {
    final fragment = html.parseFragment(toHtml(input));
    String text(dom.Node node) {
      if (node is dom.Text) return node.text;
      if (node is! dom.Element) return node.nodes.map(text).join();
      if (node.localName == 'br') return '\n';
      var value = node.nodes.map(text).join();
      if (node.localName == 'a') {
        final href = node.attributes['href'] ?? '';
        if (href.isNotEmpty && value.trim() != href) value = '$value ($href)';
      }
      if ({'p', 'div', 'li', 'blockquote', 'pre'}.contains(node.localName)) {
        value += '\n';
      }
      return value;
    }

    return fragment.nodes.map(text).join().trim();
  }
}
