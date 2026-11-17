// A personal finance read that fails is not an empty list.
//
// This page used to answer every failed read with "No debts tracked yet." and a snackbar. A person who had tracked debts
// then read that they had none, and the sentence explaining why slid away while they were still reading it. These tests
// hold the three states apart: loading, what went wrong, and genuinely nothing there.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/personal_finance/presentation/personal_finance_page.dart';

class _Reads implements PersonalFinanceReads {
  _Reads({this.debtFailure});

  final Object? debtFailure;

  @override
  Future<List<Map<String, dynamic>>> recurring() async => [];

  @override
  Future<List<Map<String, dynamic>>> debts() async {
    if (debtFailure != null) throw debtFailure!;
    return [];
  }

  @override
  Future<Map<String, dynamic>> portfolio() async => {};
}

Widget _page(PersonalFinanceReads reads) =>
    MaterialApp(home: PersonalFinancePage(reads: reads));

void main() {
  testWidgets(
    'a failed read says so, with the reason, instead of claiming there are no debts',
    (tester) async {
      await tester.pumpWidget(
        _page(
          _Reads(debtFailure: Exception('the engine could not be reached')),
        ),
      );
      await tester.pumpAndSettle();

      // the recurring tab answered: nothing there, and that is the truth
      expect(find.text('No recurring transactions yet.'), findsOneWidget);

      await tester.tap(find.text('Debts'));
      await tester.pumpAndSettle();

      expect(
        find.text('Your debts could not be read just now.'),
        findsOneWidget,
        reason:
            'the read did not answer, so the page must not speak as if it had',
      );
      expect(
        find.text('the engine could not be reached'),
        findsOneWidget,
        reason:
            'the engine\'s own words are passed on, without the exception wrapper',
      );
      expect(find.text('No debts tracked yet.'), findsNothing);
      expect(find.text('Try again'), findsOneWidget);
    },
  );

  testWidgets('a read that answered and found nothing says only that', (
    tester,
  ) async {
    await tester.pumpWidget(_page(_Reads()));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Debts'));
    await tester.pumpAndSettle();

    expect(find.text('No debts tracked yet.'), findsOneWidget);
    expect(find.text('Your debts could not be read just now.'), findsNothing);
    expect(find.text('Try again'), findsNothing);
  });

  testWidgets('Try again keeps the person on the section they were reading', (
    tester,
  ) async {
    await tester.pumpWidget(_page(_Reads(debtFailure: Exception('timed out'))));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Debts'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Try again'));
    await tester.pumpAndSettle();

    expect(
      find.text('Your debts could not be read just now.'),
      findsOneWidget,
      reason:
          'the second read failed too, and the page must still be the debts section',
    );
    expect(find.text('No recurring transactions yet.'), findsNothing);
  });
}
