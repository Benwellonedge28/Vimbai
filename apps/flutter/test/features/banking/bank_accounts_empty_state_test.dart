// A Book with no bank account linked should say what this list is for, and a failed read must not read as "none".
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/banking/data/banking_api_service.dart';
import 'package:vimbai_mobile_client/features/banking/models/banking_models.dart';
import 'package:vimbai_mobile_client/features/banking/presentation/bank_accounts_page.dart';

/// An engine that holds nothing, or one that refuses to answer.
class _BankEngine extends BankingApiService {
  _BankEngine({this.refuse = false});

  final bool refuse;
  int reads = 0;
  String? lastAdded;

  @override
  Future<List<BankConnection>> getConnections() async {
    reads++;
    if (refuse) throw Exception('the bank feed is unreachable');
    return const [];
  }

  @override
  Future<BankConnection> connectBank({
    required String accountName,
    String last4 = '',
    String accountType = 'checking',
  }) async {
    lastAdded = accountName;
    return BankConnection(
      id: 'conn-1',
      accountName: accountName,
      last4: last4,
      accountType: accountType,
      createdAt: DateTime(2026, 11, 17),
    );
  }
}

void main() {
  testWidgets('no bank account says what the list is for, and adds one', (
    tester,
  ) async {
    final engine = _BankEngine();
    await tester.pumpWidget(
      MaterialApp(home: BankAccountsPage(service: engine)),
    );
    await tester.pumpAndSettle();

    expect(find.text('No bank accounts yet'), findsOneWidget);
    expect(
      find.textContaining('nothing here logs in to your bank'),
      findsOneWidget,
    );
    expect(find.text('Add the first account'), findsOneWidget);

    await tester.tap(find.text('Add the first account'));
    await tester.pumpAndSettle();

    // No secret is asked of a person who has none, and only four digits are taken.
    expect(find.text('Add a bank account'), findsOneWidget);
    expect(find.text('Bank API Key'), findsNothing);
    expect(find.text('Account name'), findsOneWidget);
    expect(find.text('Last four digits (optional)'), findsOneWidget);

    await tester.enterText(
      find.widgetWithText(TextFormField, 'Account name'),
      'CBZ current',
    );
    await tester.tap(find.text('Add it to this Book'));
    await tester.pumpAndSettle();

    expect(engine.lastAdded, 'CBZ current');
    expect(engine.reads, 2);
  });

  testWidgets('a failed read is named, with the reason, not called empty', (
    tester,
  ) async {
    final engine = _BankEngine(refuse: true);
    await tester.pumpWidget(
      MaterialApp(home: BankAccountsPage(service: engine)),
    );
    await tester.pumpAndSettle();

    expect(
      find.text('Your bank accounts could not be read just now.'),
      findsOneWidget,
    );
    expect(find.textContaining('the bank feed is unreachable'), findsOneWidget);
    expect(find.text('No bank accounts yet'), findsNothing);

    await tester.tap(find.text('Try again'));
    await tester.pumpAndSettle();
    expect(engine.reads, 2);
  });
}
