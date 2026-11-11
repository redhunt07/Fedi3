import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:fedi3/state/app_state.dart';
import 'package:fedi3/state/config_store.dart';
import 'package:fedi3/ui/screens/first_run_screen.dart';
import '../support/fake_core.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  testWidgets(
      'locked keyring shows recovery, blocks onboarding and retries existing config',
      (tester) async {
    await setUpPumpEnv(tester);
    const raw = '{"username":"alice","_secretRef":"missing-vault-record"}';
    ConfigStore.configFile().writeAsStringSync(raw);
    final app = (await tester.runAsync(
        () => AppState.load(core: FakeCoreRuntime(), watchdogEnabled: false)))!;
    expect(app.configCorrupt, isFalse);
    expect(app.credentialsUnavailable, isTrue);
    await pumpScreen(tester, FirstRunScreen(appState: app));
    expect(find.textContaining('Unlock your system keyring'), findsOneWidget);
    expect(find.byIcon(Icons.person_add_alt_1), findsNothing);
    expect(ConfigStore.configFile().readAsStringSync(), raw);
    ConfigStore.configFile()
        .writeAsStringSync(jsonEncode(buildTestConfig().toJson()));
    await tester.runAsync(() async {
      await tester.tap(find.text('Retry'));
      for (var i = 0; i < 100 && app.credentialsUnavailable; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
    });
    await tester.pumpAndSettle();
    expect(app.config?.username, 'alice');
    expect(app.credentialsUnavailable, isFalse);
    app.dispose();
  });
}
