import 'package:flutter_test/flutter_test.dart';
import 'package:fedi3/model/ui_prefs.dart';
import 'package:fedi3/state/peer_presence_store.dart';

void main() {
  test('PR-01 presence parser accepts the four statuses', () {
    expect(presenceFromString('online'), PresenceStatus.online);
    expect(presenceFromString('BUSY'), PresenceStatus.busy);
    expect(presenceFromString('afk'), PresenceStatus.afk);
    expect(presenceFromString('offline'), PresenceStatus.offline);
    expect(presenceFromString('unknown'), PresenceStatus.offline);
    expect(presenceFromString(null), PresenceStatus.offline);
  });

  test('PR-02 ui prefs round-trip keeps the selected presence', () {
    final prefs = UiPrefs.defaults().copyWith(presenceStatus: 'busy');
    final restored = UiPrefs.fromJson(prefs.toJson());
    expect(restored.presenceStatus, 'busy');
  });

  test('PR-03 invalid stored presence falls back to online', () {
    final raw = UiPrefs.defaults().toJson()..['presenceStatus'] = 'nope';
    expect(UiPrefs.fromJson(raw).presenceStatus, 'online');
  });
}
