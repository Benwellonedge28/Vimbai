import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/account_picker_sheet.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entry_form_page.dart';

Account _account(String number, String name, String type) => Account(
  accountNumber: number,
  accountName: name,
  accountType: type,
  normalBalance: 'debit',
);

/// An engine that hands the form a chart without a phone.
class _ChartService extends AccountingApiService {
  _ChartService(this.accounts);

  final List<Account> accounts;

  @override
  Future<List<Account>> getAllAccounts() async => accounts;
}

/// connectivity_plus talks to the platform, which a test does not have; the form only ever asks which it is on.
void _answerConnectivity() {
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  messenger.setMockMethodCallHandler(
    const MethodChannel('dev.fluttercommunity.plus/connectivity'),
    (call) async => <String>['wifi'],
  );
  messenger.setMockStreamHandler(
    const EventChannel('dev.fluttercommunity.plus/connectivity_status'),
    MockStreamHandler.inline(onListen: (arguments, events) {}),
  );
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final chart = [
    _account('1000', 'Bank', 'asset'),
    _account('4000', 'Sales', 'income'),
    _account('6100', 'Rent Expense', 'expense'),
  ];

  testWidgets('the chart is offered by number and name, and search narrows it', (
    tester,
  ) async {
    // A Book opens with the chart it takes to write in it, so the numbers exist before the person has any reason to
    // know them. The picker is what stops the first entry becoming a hunt for codes.
    Account? chosen;
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: Center(
              child: ElevatedButton(
                onPressed: () async {
                  chosen = await showAccountPicker(context, accounts: chart);
                },
                child: const Text('open'),
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();

    expect(find.text('1000 - Bank'), findsOneWidget);
    expect(find.text('6100 - Rent Expense'), findsOneWidget);

    await tester.enterText(find.byType(TextField), 'rent');
    await tester.pumpAndSettle();
    expect(find.text('6100 - Rent Expense'), findsOneWidget);
    expect(find.text('1000 - Bank'), findsNothing);

    await tester.tap(find.text('6100 - Rent Expense'));
    await tester.pumpAndSettle();
    expect(chosen?.accountNumber, '6100');
  });

  testWidgets('a Book with no accounts says so instead of showing nothing', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: Center(
              child: ElevatedButton(
                onPressed: () => showAccountPicker(context, accounts: const []),
                child: const Text('open'),
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();

    expect(find.textContaining('no accounts yet'), findsOneWidget);
    expect(find.textContaining('still type a number'), findsOneWidget);
  });

  testWidgets('a line takes its account from the chart, and the name shows', (
    tester,
  ) async {
    _answerConnectivity();
    await tester.pumpWidget(
      MaterialApp(home: JournalEntryFormPage(service: _ChartService(chart))),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.widgetWithText(ElevatedButton, 'Add Journal Line'));
    await tester.pumpAndSettle();
    // before choosing, the line does not pretend to know which account it means
    expect(find.text('Bank'), findsNothing);

    await tester.tap(find.byTooltip('Choose an account'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('1000 - Bank'));
    await tester.pumpAndSettle();

    // the name under the field is the proof the number arrived: the field is not asked to describe itself
    expect(find.text('Bank'), findsOneWidget);
  });
}
