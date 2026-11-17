// A failed read is the moment a stranger is most likely to give up. These hold what the screen says at that moment:
// what could not be read, the engine's reason in plain words, and one action that can help.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';

void main() {
  testWidgets('it names what could not be read and repeats the reason', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CouldNotRead(
            what: 'The trial balance',
            error: Exception('the engine is not answering'),
          ),
        ),
      ),
    );

    expect(
      find.text('The trial balance could not be read just now.'),
      findsOneWidget,
    );
    expect(find.text('the engine is not answering'), findsOneWidget);
    expect(find.text('Try again'), findsNothing);
  });

  testWidgets('the exception scaffold never reaches the person', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CouldNotRead(
            what: 'The entries',
            error: Exception('a Book must be named'),
          ),
        ),
      ),
    );

    expect(find.text('a Book must be named'), findsOneWidget);
    expect(find.textContaining('Exception:'), findsNothing);
  });

  testWidgets('an offered retry runs', (tester) async {
    var asked = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CouldNotRead(
            what: 'The balance sheet',
            error: Exception('gone'),
            onRetry: () => asked++,
          ),
        ),
      ),
    );

    await tester.tap(find.text('Try again'));
    await tester.pump();
    expect(asked, 1);
  });

  testWidgets('a read that failed without a reason still reads as a sentence', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: CouldNotRead(what: 'The cash flow statement')),
      ),
    );

    expect(find.text('no reason given'), findsOneWidget);
  });

  test("the reason keeps the engine's wording", () {
    expect(
      CouldNotRead.reasonOf(Exception('amount must be positive')),
      'amount must be positive',
    );
    expect(CouldNotRead.reasonOf(null), 'no reason given');
  });
}
