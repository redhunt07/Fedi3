import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/pairing_screen.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  for (final status in [401, 403, 413]) {
    testWidgets('pairing upload $status handles auth and oversized history',
        (tester) async {
      final dir = await setUpPumpEnv(tester);
      final app = buildTestAppState(config: buildTestConfig());
      final client = FakeHttpClient();
      var uploads = 0;
      client.json('GET', '/_fedi3/identity/export', {
        'identity_private_key_pem': 'test-identity',
        'chat_identity_public_b64': 'public',
        'chat_identity_secret_b64': 'private',
      });
      client.json('GET', '/_fedi3/chat/snapshot', {'v': 1, 'messages': []});
      client.handler = (request) {
        if (request.method == 'PUT') {
          uploads++;
          return FakeHttpResponse(
              statusCode: uploads == 1 ? status : 204,
              body: 'untrusted-server-detail');
        }
        return client.routes['${request.method} ${request.url.path}'] ??
            FakeHttpResponse(statusCode: 404);
      };
      CoreHttp.useTestClient(client);
      try {
        await tester.runAsync(() => app.startCore());
        await pumpScreen(tester, PairingScreen(appState: app));
        for (var i = 0; i < 20; i++) {
          await tester.runAsync(
              () => Future<void>.delayed(const Duration(milliseconds: 20)));
          await tester.pump(const Duration(milliseconds: 50));
          if (uploads > 0 &&
              find.byType(CircularProgressIndicator).evaluate().isEmpty) {
            break;
          }
        }
        expect(find.byType(CircularProgressIndicator), findsNothing);
        expect(find.textContaining('untrusted-server-detail'), findsNothing);
        if (status != 413) {
          expect(uploads, 1);
          expect(find.byType(QrImageView), findsNothing);
          expect(find.textContaining('relay rejected'), findsOneWidget);
        } else {
          expect(uploads, 2);
          expect(find.byType(QrImageView), findsOneWidget);
        }
        await tester.pumpWidget(const SizedBox.shrink());
      } finally {
        app.dispose();
        CoreHttp.reset();
        resetStoreOverrides();
        dir.deleteSync(recursive: true);
      }
    });
  }
}
