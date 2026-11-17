// An account with nothing brought in yet should say so, and a failed read must not read as "no transactions".
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/banking/data/banking_api_service.dart';
import 'package:vimbai_mobile_client/features/banking/models/banking_models.dart';
import 'package:vimbai_mobile_client/features/banking/presentation/bank_account_detail_page.dart';

/// An engine holding no rows for this account, or one that refuses to answer.
class _RowEngine extends BankingApiService {
  _RowEngine({this.refuse = false});

  final bool refuse;
  int reads = 0;

  @override
  Future<List<BankTransaction>> getTransactions(
    String connectionId, {
    int limit = 50,
    int offset = 0,
  }) async {
    reads++;
    if (refuse) throw Exception('the bank feed is unreachable');
    return const [];
  }
}

BankConnection _account() => BankConnection(
  id: 'conn-1',
  accountName: 'CBZ current',
  last4: '0123',
  createdAt: DateTime(2026, 11, 17),
);

void main() {
  testWidgets('an account with no rows explains where rows come from', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: BankAccountDetailPage(
          connection: _account(),
          service: _RowEngine(),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('CBZ current'), findsOneWidget);
    expect(find.text('Account ending 0123'), findsOneWidget);
    expect(
      find.textContaining('Rows appear when a statement is brought in'),
      findsOneWidget,
    );
  });

  testWidgets('a failed read is named, with the reason, not called empty', (
    tester,
  ) async {
    final engine = _RowEngine(refuse: true);
    await tester.pumpWidget(
      MaterialApp(
        home: BankAccountDetailPage(connection: _account(), service: engine),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.text('The rows from this account could not be read just now.'),
      findsOneWidget,
    );
    expect(find.textContaining('the bank feed is unreachable'), findsOneWidget);
    expect(
      find.textContaining('Rows appear when a statement is brought in'),
      findsNothing,
    );

    await tester.tap(find.text('Try again'));
    await tester.pumpAndSettle();
    expect(engine.reads, 2);
  });
}
