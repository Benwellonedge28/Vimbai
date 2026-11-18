// Proof nothing has changed: the engine's two real answers, and the page that must not confuse them.
//
// The JSON in this file is copied from a live engine: the intact answer for three posted entries, the empty book, and the two
// errors a hand-edited entry produced (the entry itself stops matching, and the entry after it expects a different
// predecessor). A parser tested against invented shapes would pass while the screen showed nothing.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/integrity/data/ledger_integrity.dart';
import 'package:vimbai_mobile_client/features/integrity/presentation/ledger_integrity_page.dart';

const _intact = {
  'valid': true,
  'entries_checked': 3,
  'head_hash':
      '2b433d63c9850d7049789b6498e3d5d6f8d8a6b8b0ebc3e5f862c7e29751369a',
  'errors': <dynamic>[],
  'legacy_stamped': <dynamic>[],
};

const _empty = {
  'valid': true,
  'entries_checked': 0,
  'head_hash':
      '0000000000000000000000000000000000000000000000000000000000000000',
  'errors': <dynamic>[],
  'legacy_stamped': <dynamic>[],
};

const _tampered = {
  'valid': false,
  'entries_checked': 3,
  'head_hash':
      '2b433d63c9850d7049789b6498e3d5d6f8d8a6b8b0ebc3e5f862c7e29751369a',
  'errors': [
    {
      'code': 'hash_mismatch',
      'detail': {
        'entry_id': '18e92148d6bc921800000000110051fa0f4a',
        'expected':
            '8ab00d0646d129e6f902da8f8183fdffb7f9b35cc2dcad81f47aabf1e3fb8c62',
        'actual':
            '64cfde1b1da5e3907fcc73f5b5070d9a87d1e55ce5e4ac09519b370e50cb7e68',
      },
    },
    {
      'code': 'broken_link',
      'detail': {
        'entry_id': '18e92148d8f51ab50000000016002e9c09b5',
        'prev_in_chain':
            '8ab00d0646d129e6f902da8f8183fdffb7f9b35cc2dcad81f47aabf1e3fb8c62',
        'stored_prev':
            '64cfde1b1da5e3907fcc73f5b5070d9a87d1e55ce5e4ac09519b370e50cb7e68',
      },
    },
  ],
  'legacy_stamped': <dynamic>[],
};

class _Answers implements LedgerIntegritySource {
  _Answers(this.json);

  final Map<String, dynamic> json;
  int calls = 0;

  @override
  Future<LedgerIntegrity> check() async {
    calls++;
    return LedgerIntegrity.fromJson(json);
  }
}

class _Fails implements LedgerIntegritySource {
  @override
  Future<LedgerIntegrity> check() async =>
      throw Exception('the engine answered 503 while it restarts');
}

void main() {
  group('the engine’s answer', () {
    test(
      'an intact book counts its entries and keeps the head fingerprint',
      () {
        final integrity = LedgerIntegrity.fromJson(_intact);
        expect(integrity.isIntact, isTrue);
        expect(integrity.isProofOfNothing, isFalse);
        expect(integrity.entriesChecked, 3);
        expect(integrity.problems, isEmpty);
        expect(integrity.shortHeadHash, '2b433d63c985…');
        expect(integrity.shortHeadHash.length, 13);
      },
    );

    test('an empty book is not an intact book', () {
      final integrity = LedgerIntegrity.fromJson(_empty);
      expect(integrity.valid, isTrue);
      expect(integrity.isProofOfNothing, isTrue);
      expect(integrity.isIntact, isFalse);
    });

    test('a broken chain names the entry, and the entry after it', () {
      final integrity = LedgerIntegrity.fromJson(_tampered);
      expect(integrity.isIntact, isFalse);
      expect(integrity.problems.length, 2);
      expect(integrity.problems.first.code, 'hash_mismatch');
      expect(
        integrity.problems.first.entryId,
        '18e92148d6bc921800000000110051fa0f4a',
      );
      expect(
        integrity.problems.first.meaning,
        contains('no longer matches the fingerprint'),
      );
      expect(
        integrity.problems.last.meaning,
        contains('expecting a different entry before it'),
      );
    });

    test('a code the app does not know is passed on, not guessed at', () {
      final integrity = LedgerIntegrity.fromJson(const {
        'valid': false,
        'entries_checked': 1,
        'head_hash': 'abc',
        'errors': [
          {
            'code': 'something_new',
            'detail': {'entry_id': 'e-1'},
          },
        ],
        'legacy_stamped': <dynamic>[],
      });
      expect(integrity.problems.single.meaning, contains('something_new'));
    });

    test('a short fingerprint is left whole', () {
      final integrity = LedgerIntegrity.fromJson(const {
        'valid': true,
        'entries_checked': 1,
        'head_hash': 'abc123',
        'errors': <dynamic>[],
        'legacy_stamped': <dynamic>[],
      });
      expect(integrity.shortHeadHash, 'abc123');
    });
  });

  group('the page', () {
    Widget page(LedgerIntegritySource source) =>
        MaterialApp(home: LedgerIntegrityPage(source: source));

    testWidgets('an intact book says so and shows the newest fingerprint', (
      tester,
    ) async {
      await tester.pumpWidget(page(_Answers(_intact)));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('All 3 entries link together'),
        findsOneWidget,
      );
      expect(find.text('2b433d63c985…'), findsOneWidget);
      expect(
        find.textContaining('Nothing has changed since it was written'),
        findsOneWidget,
      );
    });

    testWidgets('one entry is said in the singular', (tester) async {
      await tester.pumpWidget(
        page(
          _Answers(const {
            'valid': true,
            'entries_checked': 1,
            'head_hash': 'abc123',
            'errors': <dynamic>[],
            'legacy_stamped': <dynamic>[],
          }),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('All 1 entry links together'), findsOneWidget);
    });

    testWidgets('an empty book says there is nothing to prove', (tester) async {
      await tester.pumpWidget(page(_Answers(_empty)));
      await tester.pumpAndSettle();

      expect(find.textContaining('nothing to prove yet'), findsOneWidget);
      expect(find.textContaining('link together'), findsNothing);
    });

    testWidgets('a broken chain leads with what was found', (tester) async {
      await tester.pumpWidget(page(_Answers(_tampered)));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('no longer match what was written'),
        findsOneWidget,
      );
      expect(find.textContaining('2 problems were found'), findsOneWidget);
      expect(
        find.textContaining('18e92148d6bc921800000000110051fa0f4a'),
        findsOneWidget,
      );
      expect(find.textContaining('link together'), findsNothing);
    });

    testWidgets('a failed read never shows a tick', (tester) async {
      await tester.pumpWidget(page(_Fails()));
      await tester.pumpAndSettle();

      expect(find.byType(CouldNotRead), findsOneWidget);
      expect(find.textContaining('503'), findsOneWidget);
      expect(find.textContaining('link together'), findsNothing);
    });

    testWidgets('checking again asks the engine again', (tester) async {
      final answers = _Answers(_intact);
      await tester.pumpWidget(page(answers));
      await tester.pumpAndSettle();
      expect(answers.calls, 1);

      await tester.tap(find.byTooltip('Check again'));
      await tester.pumpAndSettle();
      expect(answers.calls, 2);
    });
  });
}
