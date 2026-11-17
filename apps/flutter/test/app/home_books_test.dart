import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/app/home_page.dart';
import 'package:vimbai_mobile_client/features/books/models/book_models.dart';

Widget _home(Future<List<VBook>> Function() readBooks) =>
    MaterialApp(home: HomePage(readBooks: readBooks, backgroundWork: false));

/// Take the page away and let any one-shot timeout inside it fire, so no timer is left when the test ends.
Future<void> _close(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
  await tester.pump(const Duration(minutes: 2));
}

void main() {
  testWidgets('Books that could not be read say the list may be short', (
    tester,
  ) async {
    await tester.pumpWidget(
      _home(() async => throw Exception('the engine is unreachable')),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Books could not be read just now'),
      findsOneWidget,
    );
    expect(
      find.textContaining('They are kept on the server, not lost'),
      findsOneWidget,
    );
    expect(find.text('Try again'), findsOneWidget);
    // not claiming the account is empty when the read failed
    expect(find.textContaining('No Books yet'), findsNothing);
    await _close(tester);
  });

  testWidgets('an empty account offers the first Book where it is said', (
    tester,
  ) async {
    await tester.pumpWidget(_home(() async => <VBook>[]));
    await tester.pumpAndSettle();

    expect(find.text('No Books yet - create your first'), findsOneWidget);
    expect(find.textContaining('could not be read'), findsNothing);
    await _close(tester);
  });
}
