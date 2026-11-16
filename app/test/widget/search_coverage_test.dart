import 'package:fedi3/core/core_http.dart';
import 'package:fedi3/ui/screens/search_screen.dart';
import 'package:fedi3/ui/widgets/network_error_card.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/fake_core.dart';
import '../support/fake_http.dart';
import '../support/pump_app.dart';
import '../support/test_config.dart';

void main() {
  for (final tab in [SearchTab.posts, SearchTab.users]) {
    testWidgets(
        '$tab search shows available results while relay coverage is incomplete',
        (tester) async {
      await setUpPumpEnv(tester);
      final client = FakeHttpClient(
          defaultResponse: FakeHttpResponse(body: '{"items":[]}'));
      client.handler = (request) =>
          request.url.queryParameters['consistency'] == 'full'
              ? FakeHttpResponse(
                  statusCode: 503, body: 'search coverage incomplete')
              : FakeHttpResponse(body: '{"items":[],"next":null}');
      CoreHttp.useTestClient(client);
      addTearDown(CoreHttp.reset);
      final app =
          buildTestAppState(config: buildTestConfig(), core: FakeCoreRuntime());
      await app.startCore();
      await pumpScreen(tester,
          SearchScreen(appState: app, initialQuery: 's', initialTab: tab));
      await tester.pumpAndSettle();
      final path = tab == SearchTab.posts
          ? '/_fedi3/search/notes'
          : '/_fedi3/search/users';
      expect(client.requestsFor('GET', path).last.url.queryParameters['source'],
          'all');
      expect(
          client
              .requestsFor('GET', path)
              .last
              .url
              .queryParameters['consistency'],
          'best');
      expect(find.byType(NetworkErrorCard), findsNothing);
    });
  }
}
