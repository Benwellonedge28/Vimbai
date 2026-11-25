// Clearing money out of suspense.
//
// What these pin: that the screen says what each side does to the books before it is posted, that the side the person
// chose is the side that is sent, that a missing day, account or figure of nothing is refused in fixing words before the
// engine is asked, that the engine's own refusal reaches the person, that what was cleared is shown once it is done, and
// that the screen says plainly that nothing moves money.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/suspense/data/suspense_clearing.dart';
import 'package:vimbai_mobile_client/features/suspense/presentation/clear_suspense_page.dart';

/// Stands in for the engine's suspense journal: it remembers what it was asked to do.
class _FakeClearing extends SuspenseClearingService {
  _FakeClearing({this.refusal = ''}) : super.fresh();

  /// When set, the next write is refused in these words, as the engine would.
  String refusal;

  final List<Map<String, Object?>> posted = [];

  @override
  Future<SuspenseClearing> clear({
    required String entryDate,
    required String accountNumber,
    required String amount,
    required String side,
    String reason = '',
    String suspenseAccount = '',
  }) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    posted.add({
      'entry_date': entryDate,
      'account_number': accountNumber,
      'amount': amount,
      'side': side,
      'reason': reason,
    });
    return SuspenseClearing(
      entryDate: entryDate,
      amount: amount,
      side: side,
      accountNumber: accountNumber,
      accountName: accountNumber == '4000' ? 'Sales' : 'Expenses',
      reference: 'SUS-$entryDate-$accountNumber-$amount',
      journalEntryId: 'entry-1',
    );
  }
}

/// Opens the screen on a viewport tall enough to hold the whole form, so nothing has to be scrolled to be pressed.
Future<void> _open(
  WidgetTester tester,
  SuspenseClearingService clearing,
) async {
  tester.view.physicalSize = const Size(1200, 3200);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(home: ClearSuspensePage(clearing: clearing)),
  );
}

/// Fills the form the way a person would, leaving out whatever a test wants left out.
Future<void> _fill(
  WidgetTester tester, {
  String? day,
  String? account,
  String? figure,
  String? reason,
}) async {
  if (day != null) await tester.enterText(find.byType(TextField).at(0), day);
  if (account != null) {
    await tester.enterText(find.byType(TextField).at(1), account);
  }
  if (figure != null) {
    await tester.enterText(find.byType(TextField).at(2), figure);
  }
  if (reason != null)
    await tester.enterText(find.byType(TextField).at(3), reason);
  await tester.pumpAndSettle();
}

Future<void> _clear(WidgetTester tester) async {
  await tester.tap(find.text('Clear it out of suspense'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('the screen says what each side does to the books', (
    tester,
  ) async {
    await _open(tester, _FakeClearing());

    expect(find.textContaining('Suspense holds an amount'), findsOneWidget);
    // crediting the account means money came in, and it says so before anything is posted
    expect(find.textContaining('It was earned or owed'), findsOneWidget);
    expect(find.textContaining('money came in'), findsOneWidget);
    expect(
      find.textContaining(
        'suspense is a bookkeeping place, not an account anybody holds',
      ),
      findsOneWidget,
    );
  });

  testWidgets(
    'a clearing states the day, the account, the amount and the side',
    (tester) async {
      final clearing = _FakeClearing();
      await _open(tester, clearing);

      await _fill(
        tester,
        day: '2026-11-20',
        account: '4000',
        figure: '500.00',
        reason: 'customer identified',
      );
      await _clear(tester);

      expect(clearing.posted, hasLength(1));
      expect(clearing.posted.single, {
        'entry_date': '2026-11-20',
        'account_number': '4000',
        'amount': '500.00',
        'side': 'credit',
        'reason': 'customer identified',
      });
      // and what it did is shown, with the entry's own reference
      expect(find.textContaining('SUS-2026-11-20-4000-500.00'), findsOneWidget);
      expect(find.textContaining('suspense debited'), findsOneWidget);
    },
  );

  testWidgets('choosing that it was spent sends the other side', (
    tester,
  ) async {
    final clearing = _FakeClearing();
    await _open(tester, clearing);

    await tester.tap(find.text('It was spent'));
    await tester.pumpAndSettle();
    expect(find.textContaining('money went out'), findsOneWidget);

    await _fill(tester, account: '5000', figure: '80');
    await _clear(tester);

    expect(clearing.posted.single['side'], 'debit');
    expect(find.textContaining('suspense credited'), findsOneWidget);
  });

  testWidgets(
    'a missing day, account or figure is refused before the engine is asked',
    (tester) async {
      final clearing = _FakeClearing();
      await _open(tester, clearing);

      await _fill(tester, day: '20-11-2026');
      await _clear(tester);
      expect(find.textContaining('Say the day'), findsOneWidget);

      await _fill(tester, day: '2026-11-20');
      await _clear(tester);
      expect(
        find.textContaining('which account the amount belongs in'),
        findsOneWidget,
      );

      await _fill(tester, account: '4000', figure: '0');
      await _clear(tester);
      expect(find.textContaining('more than nothing'), findsOneWidget);

      expect(clearing.posted, isEmpty);
    },
  );

  testWidgets("the engine's own refusal reaches the person", (tester) async {
    final clearing = _FakeClearing(
      refusal:
          'Clearing suspense into itself moves nothing: say the account the amount belongs in.',
    );
    await _open(tester, clearing);

    await _fill(tester, account: '9999', figure: '10');
    await _clear(tester);

    expect(find.textContaining('moves nothing'), findsOneWidget);
    expect(find.text('Nothing yet.'), findsOneWidget);
  });
}
