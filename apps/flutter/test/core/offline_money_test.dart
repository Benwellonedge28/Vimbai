import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/local_db/database_helper.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
    // a scratch file, so the test never touches a real phone's store
    databaseFactory.setDatabasesPath(
      p.join(
        Directory.systemTemp.createTempSync('vimbai_offline_money').path,
        '',
      ),
    );
  });

  test('a thirty-digit amount survives the phone\'s own store exactly', () async {
    // This row is what an unsynced entry is sent to the engine from. The table used to hold money in a REAL column,
    // which silently rewrites anything a double cannot hold, so a figure could come out of the queue different from the
    // one that was typed.
    final amount = ExactAmount.parse('123456789012345678901234567890.123456');
    final entry = JournalEntry(
      id: 'entry-big-money',
      entryDate: DateTime(2026, 11, 17),
      description: 'A very large figure',
      sourceModule: 'MobileApp',
      status: 'posted',
      lines: [
        JournalLine(accountNumber: '1000', debit: amount),
        JournalLine(accountNumber: '4000', credit: amount),
      ],
    );

    final helper = DatabaseHelper();
    await helper.insertJournalEntry(entry);

    final queue = await helper.getUnsyncedJournalEntries();
    final saved = queue.firstWhere((e) => e.id == 'entry-big-money');
    expect(saved.lines.first.debit, amount);
    expect(saved.lines.first.debit.toLedgerString(), amount.toLedgerString());
    expect(saved.lines.last.credit, amount);
    // the pair is still balanced after the trip through the store, which is the point of keeping the digits
    expect(saved.lines.first.debit - saved.lines.last.credit, ExactAmount.zero);
  });

  test('one minor unit is not rounded away on the way in', () async {
    final entry = JournalEntry(
      id: 'entry-minor-unit',
      entryDate: DateTime(2026, 11, 17),
      description: 'One millionth',
      sourceModule: 'MobileApp',
      status: 'posted',
      lines: [
        JournalLine(
          accountNumber: '1000',
          debit: ExactAmount.parse('0.000001'),
        ),
        JournalLine(
          accountNumber: '4000',
          credit: ExactAmount.parse('0.000001'),
        ),
      ],
    );

    final helper = DatabaseHelper();
    await helper.insertJournalEntry(entry);

    final queue = await helper.getUnsyncedJournalEntries();
    final saved = queue.firstWhere((e) => e.id == 'entry-minor-unit');
    expect(saved.lines.first.debit, ExactAmount.parse('0.000001'));
    expect(saved.lines.first.debit.isZero, isFalse);
  });
}
