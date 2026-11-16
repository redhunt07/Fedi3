import 'package:fedi3/model/note_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('attachments accept a single object and URL link alternatives', () {
    final parsed = NoteAttachment.parseList({
      'type': 'Document',
      'mediaType': 'image/jpeg',
      'url': [
        {'type': 'Link', 'href': 'https://remote.test/photo.jpg'}
      ],
    });
    expect(parsed, hasLength(1));
    expect(parsed.single.url, 'https://remote.test/photo.jpg');
    expect(parsed.single.mediaType, 'image/jpeg');
    expect(NoteAttachment.parseList(null), isEmpty);
  });
}
