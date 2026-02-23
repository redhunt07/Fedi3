import 'dart:convert';
import 'package:fedi3/model/note_models.dart';

void main() {
  final raw = jsonDecode(r'''
  {
    "type": "Announce",
    "id": "https://feddit.it/activities/announce/create/c23d1fef",
    "actor": "https://feddit.it/c/pirati",
    "object": {
      "type": "Page",
      "id": "https://poliversity.it/users/macfranc/statuses/115046469518849211",
      "attributedTo": "https://poliversity.it/users/macfranc",
      "content": "<p>hello</p>",
      "published": "2025-08-17T22:30:10Z",
      "mediaType": "text/html"
    }
  }
  ''') as Map<String, dynamic>;

  final item = TimelineItem.tryFromActivity(raw);
  print('item: ${item != null}');
  if (item != null) {
    print('note id: ${item.note.id} content: ${item.note.contentHtml}');
  }
}
