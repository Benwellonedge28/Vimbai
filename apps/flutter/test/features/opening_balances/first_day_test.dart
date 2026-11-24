// The first day of a business's own books.
//
// What these pin: that a business states what it already held and where it came from rather than being asked for debits
// and credits, that the two sides are compared while they are being typed and a set that does not agree is refused before
// anything is sent, that a Book with a first day reads it back instead of offering a second one, and that the engine's own
// refusals reach the person in words they can act on.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/opening_balances/data/opening_balances_posting.dart';
import 'package:vimbai_mobile_client/features/opening_balances/presentation/your_first_day_page.dart';

/// The Book's chart: what a business arriving here already keeps.
class _FakeLedger extends AccountingApiService {
  @override
  Future<List<Account>> getAllAccounts() async => [
    Account(
      accountNumber: '1010',
      accountName: 'Bank',
      accountType: 'asset',
      normalBalance: 'debit',
    ),
    Account(
      accountNumber: '1200',
      accountName: 'Inventory',
      accountType: 'asset',
      normalBalance: 'debit',
    ),
    Account(
      accountNumber: '2000',
      accountName: 'Accounts Payable',
      accountType: 'liability',
      normalBalance: 'credit',
    ),
    Account(
      accountNumber: '3000',
      accountName: "Owner's Equity",
      accountType: 'equity',
      normalBalance: 'credit',
    ),
  ];
}

/// Stands in for the engine's opening balances journal: it remembers what it was asked to do.
class _FakeOpening extends OpeningBalancesPostingService {
  _FakeOpening({this.stated = OpeningSet.empty, this.refusal = ''})
    : super.fresh();

  OpeningSet stated;

  /// When set, the next write is refused in these words, as the engine would.
  String refusal;

  final List<Map<String, Object?>> posted = [];

  @override
  Future<OpeningSet> read() async => stated;

  @override
  Future<OpeningSet> post({
    required String entryDate,
    required List<OpeningLine> lines,
    String notes = '',
  }) async {
    if (refusal.isNotEmpty) throw Exception(refusal);
    posted.add({
      'entry_date': entryDate,
      'notes': notes,
      'lines': [
        for (final line in lines)
          {
            'account': line.accountNumber,
            'debit': line.debit,
            'credit': line.credit,
          },
      ],
    });
    stated = OpeningSet(
      entryDate: entryDate,
      debits: '20000.00',
      credits: '20000.00',
      accounts: lines,
      journalEntryId: 'entry-1',
      notes: notes,
    );
    return stated;
  }
}

void main() {
  /// The screen as a person sees it, on a surface tall enough that the whole form is built: a phone scrolls, a test has
  /// to be able to reach every field.
  Future<void> open(WidgetTester tester, _FakeOpening opening) async {
    tester.view.physicalSize = const Size(1000, 2600);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MaterialApp(
        home: YourFirstDayPage(posting: opening, ledger: _FakeLedger()),
      ),
    );
    await tester.pumpAndSettle();
  }

  /// State one line: pick the account it belongs to, then its figure.
  ///
  /// A filled row stops saying "Choose", so the line being filled is the last one still asking: the holds lines come
  /// first in the form and the came-from lines after them.
  Future<void> state(
    WidgetTester tester, {
    required bool holds,
    required String account,
    required String figure,
  }) async {
    final asking = find.text('Choose');
    await tester.tap(holds ? asking.first : asking.last);
    await tester.pumpAndSettle();
    await tester.tap(find.text(account).last);
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Figure').at(holds ? 0 : 1),
      figure,
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'a business states what it holds and where it came from, not debits and credits',
    (tester) async {
      final opening = _FakeOpening();
      await open(tester, opening);
      await tester.pumpAndSettle();

      // the words a person uses, and the promise that nothing moves
      expect(find.text('What the business holds'), findsOneWidget);
      expect(find.text('Where it came from'), findsOneWidget);
      expect(
        find.textContaining(
          'Nothing moves: this states what the business already had',
        ),
        findsOneWidget,
      );
      expect(find.text('Open the books on this day'), findsOneWidget);
      expect(opening.posted, isEmpty);
    },
  );

  testWidgets(
    'a set whose sides agree is posted with the day, the accounts and each figure on its side',
    (tester) async {
      final opening = _FakeOpening();
      await open(tester, opening);
      await tester.pumpAndSettle();

      await tester.enterText(
        find.widgetWithText(TextField, 'The first day'),
        '2026-01-01',
      );
      await state(
        tester,
        holds: true,
        account: '1010 - Bank',
        figure: '12000.00',
      );
      await state(
        tester,
        holds: false,
        account: "3000 - Owner's Equity",
        figure: '12000.00',
      );
      await tester.pumpAndSettle();

      // the screen says the sides agree before anything is sent
      expect(find.text('The two sides agree.'), findsOneWidget);

      await tester.tap(find.text('Open the books on this day'));
      await tester.pumpAndSettle();

      expect(opening.posted, hasLength(1));
      final sent = opening.posted.single;
      expect(sent['entry_date'], '2026-01-01');
      final lines = sent['lines'] as List;
      // what it holds is a debit and where it came from is a credit: the phone says the words, the engine keeps the sides
      expect(lines.first, {
        'account': '1010',
        'debit': '12000.00',
        'credit': '',
      });
      expect(lines.last, {
        'account': '3000',
        'debit': '',
        'credit': '12000.00',
      });
    },
  );

  testWidgets(
    'a set that does not balance is refused before it is sent, saying how far apart it is',
    (tester) async {
      final opening = _FakeOpening();
      await open(tester, opening);
      await tester.pumpAndSettle();

      await state(
        tester,
        holds: true,
        account: '1010 - Bank',
        figure: '12000.00',
      );
      await state(
        tester,
        holds: false,
        account: "3000 - Owner's Equity",
        figure: '11000.00',
      );
      await tester.pumpAndSettle();

      // the difference is stated while the figures are still being typed
      expect(find.textContaining('1000.00 apart'), findsOneWidget);

      await tester.tap(find.text('Open the books on this day'));
      await tester.pumpAndSettle();

      expect(opening.posted, isEmpty);
      expect(
        find.textContaining('find the figure that is missing'),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'a line with a figure and no account is refused in fixing words',
    (tester) async {
      final opening = _FakeOpening();
      await open(tester, opening);
      await tester.pumpAndSettle();

      await tester.enterText(
        find.widgetWithText(TextField, 'Figure').at(0),
        '500.00',
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Open the books on this day'));
      await tester.pumpAndSettle();

      expect(opening.posted, isEmpty);
      expect(
        find.textContaining('has a figure but no account'),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'the engine refusing a second first day reaches the person in its own words',
    (tester) async {
      final opening = _FakeOpening(
        refusal:
            'Journal Entry with reference OPENING already exists for source OpeningBalances',
      );
      await open(tester, opening);
      await tester.pumpAndSettle();

      await state(
        tester,
        holds: true,
        account: '1010 - Bank',
        figure: '800.00',
      );
      await state(
        tester,
        holds: false,
        account: '2000 - Accounts Payable',
        figure: '800.00',
      );
      await tester.tap(find.text('Open the books on this day'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('reference OPENING already exists'),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'a Book that has a first day reads it back and offers no second one',
    (tester) async {
      final opening = _FakeOpening(
        stated: const OpeningSet(
          entryDate: '2026-01-01',
          debits: '20000.00',
          credits: '20000.00',
          accounts: [
            OpeningLine(accountNumber: '1010', debit: '12000.00', credit: ''),
            OpeningLine(accountNumber: '3000', debit: '', credit: '12000.00'),
          ],
          journalEntryId: 'entry-1',
          notes: 'From the old books',
        ),
      );
      await open(tester, opening);
      await tester.pumpAndSettle();

      expect(find.text('These books start on 2026-01-01'), findsOneWidget);
      expect(find.textContaining('20000.00 held'), findsOneWidget);
      // and no form, because there is only one first day
      expect(find.text('Open the books on this day'), findsNothing);
      expect(find.text('1010 · Bank'), findsOneWidget);
      expect(find.text('3000 · Owner\'s Equity'), findsOneWidget);
    },
  );
}
