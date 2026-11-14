import 'package:fedi3/model/profile_content.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:html/parser.dart' as html;

void main() {
  test('repair recursively escaped paragraphs and make plain URLs clickable',
      () {
    const input =
        '<p>&lt;p&gt;&amp;lt;p&amp;gt;Developer &amp;amp;amp; maintainer&amp;lt;/p&amp;gt;&lt;/p&gt;</p>';
    final output = ProfileContent.toHtml(input);
    expect(output, '<p>Developer &amp; maintainer</p>');
    expect(ProfileContent.toPlainText(output), 'Developer & maintainer');
    final field = ProfileContent.toHtml(
        '<p>&lt;p&gt;https://github.com/example&lt;/p&gt;</p>');
    expect(html.parseFragment(field).querySelector('a')?.attributes['href'],
        'https://github.com/example');
  });
  test('profile synchronization and restarting never add HTML escaping', () {
    var value = 'A & B\nhttps://example.test/?a=1&b=2';
    final canonical = ProfileContent.toHtml(value);
    for (var i = 0; i < 15; i++) {
      value = ProfileContent.toPlainText(ProfileContent.toHtml(value));
      expect(ProfileContent.toHtml(value), canonical);
    }
  });
  test(
      'preserve links and line breaks, omit executable markup and unsafe links',
      () {
    const input =
        '<p>Hello<br>world <a href="https://example.test" rel="me">Website</a></p><script>alert(1)</script><a href="javascript:alert(1)">bad</a>';
    final output = ProfileContent.toHtml(input);
    final fragment = html.parseFragment(output);
    expect(fragment.querySelector('script'), isNull);
    expect(fragment.querySelectorAll('a').last.attributes['href'], isNull);
    expect(fragment.querySelector('a')?.attributes['rel'], 'me');
    expect(ProfileContent.toPlainText(output),
        contains('Hello\nworld Website (https://example.test)'));
    expect(ProfileContent.toHtml(output), output);
  });
}
