// The first time someone opens Books there is nothing to list, and the screen used to be blank apart from a "+".
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/books/presentation/no_books_yet.dart';

void main() {
  testWidgets('a first run is told what a Book is and offered the door', (
    tester,
  ) async {
    var opened = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: NoBooksYet(onCreate: () => opened++)),
      ),
    );

    expect(find.text('You have no Books yet'), findsOneWidget);
    expect(find.textContaining('chart of accounts'), findsOneWidget);

    await tester.tap(find.text('Create your first Book'));
    await tester.pump();
    expect(opened, 1);
  });
}
