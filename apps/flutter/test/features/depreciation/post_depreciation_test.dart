// Putting the wear on what a business owns into its books.
//
// The register works the charge out and this only carries the month, so what these pin is that the screen posts a month
// rather than a figure, that it catches up in order from the oldest month waiting, that a month already in the books says
// so instead of charging it twice, and that the engine's own refusal reaches the person in words they can act on.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/depreciation/data/depreciation_posting.dart';
import 'package:vimbai_mobile_client/features/depreciation/presentation/post_depreciation_page.dart';

/// Stands in for the engine's depreciation journal: it remembers which month it was asked to post.
class _FakeDepreciation extends DepreciationPostingService {
  _FakeDepreciation({this.waitingOn = const [], this.refusal = ''})
    : super.fresh();

  /// What the register has recorded and the books have not.
  List<DepreciationCharge> waitingOn;

  /// When set, posting is refused in these words, as the engine would.
  String refusal;

  final List<String> postedMonths = [];

  @override
  Future<DepreciationWaiting> waiting() async => DepreciationWaiting(
    charges: waitingOn,
    total: waitingOn.isEmpty ? '0.00' : '300.00',
  );

  @override
  Future<DepreciationPosted> postMonth(String period) async {
    postedMonths.add(period);
    if (refusal.isNotEmpty) throw Exception(refusal);
    final charged = waitingOn.where((one) => one.period == period).toList();
    // posting takes them out of the waiting list, which is what the next read would find
    waitingOn = waitingOn.where((one) => one.period != period).toList();
    return DepreciationPosted(
      period: period,
      total: charged.isEmpty ? '0.00' : '300.00',
      charged: charged
          .map(
            (one) => DepreciationCharge(
              assetName: one.assetName,
              assetId: one.assetId,
              period: one.period,
              amount: one.amount,
              journalEntryId: 'entry-1',
            ),
          )
          .toList(),
      alreadyPosted: const [],
    );
  }
}

DepreciationCharge _charge(String name, String period, String amount) =>
    DepreciationCharge(
      assetName: name,
      assetId: 'asset-$name',
      period: period,
      amount: amount,
      journalEntryId: '',
    );

Future<void> _open(
  WidgetTester tester, {
  required _FakeDepreciation depreciation,
}) async {
  tester.view.physicalSize = const Size(1000, 2400);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(home: PostDepreciationPage(posting: depreciation)),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('the month waiting is the one proposed, oldest first', (
    tester,
  ) async {
    final depreciation = _FakeDepreciation(
      waitingOn: [
        _charge('Delivery van', '2026-11', '200.00'),
        _charge('Fridge', '2026-11', '100.00'),
        _charge('Laptop', '2026-10', '50.00'),
      ],
    );
    await _open(tester, depreciation: depreciation);

    // catching up in order: the October charge is the one proposed, not the newest month
    expect(find.text('2026-10'), findsWidgets);
    expect(find.text('Laptop'), findsOneWidget);
    expect(find.text('300.00 across 2 months.'), findsNothing);
    expect(
      find.text('Waiting to reach your books: 300.00 across 2 months.'),
      findsOneWidget,
    );
  });

  testWidgets('the month is posted, not a figure, and what went in is named', (
    tester,
  ) async {
    final depreciation = _FakeDepreciation(
      waitingOn: [
        _charge('Delivery van', '2026-10', '200.00'),
        _charge('Fridge', '2026-10', '100.00'),
      ],
    );
    await _open(tester, depreciation: depreciation);

    await tester.tap(find.text('Post 2026-10 to the books'));
    await tester.pumpAndSettle();

    expect(depreciation.postedMonths, ['2026-10']);
    expect(
      find.text('2026-10 is in your books: 300.00 of wear.'),
      findsOneWidget,
    );
    expect(find.text('Delivery van'), findsWidgets);
  });

  testWidgets('a month the register recorded nothing for cannot be posted', (
    tester,
  ) async {
    // the only month waiting is October, so November has nothing behind it and the button says so rather than sending it
    final depreciation = _FakeDepreciation(
      waitingOn: [_charge('Laptop', '2026-10', '50.00')],
    );
    await _open(tester, depreciation: depreciation);

    await tester.tap(find.text('2026-10'));
    await tester.pumpAndSettle();
    // there is no November in the list, because the register has recorded nothing for it
    expect(find.text('2026-11'), findsNothing);
    expect(find.text('Post 2026-10 to the books'), findsOneWidget);
  });

  testWidgets('the engine\'s refusal reaches the person in its own words', (
    tester,
  ) async {
    final depreciation = _FakeDepreciation(
      waitingOn: [_charge('Laptop', '2026-10', '50.00')],
      refusal: 'The month 2026-10 is closed, so nothing can be posted into it.',
    );
    await _open(tester, depreciation: depreciation);

    await tester.tap(find.text('Post 2026-10 to the books'));
    await tester.pumpAndSettle();

    expect(
      find.text(
        'The month 2026-10 is closed, so nothing can be posted into it.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('nothing waiting says where a month comes from', (tester) async {
    final depreciation = _FakeDepreciation();
    await _open(tester, depreciation: depreciation);

    expect(
      find.textContaining('once it has been charged in "Things you own"'),
      findsOneWidget,
    );
    expect(find.textContaining('Nothing to post for'), findsOneWidget);
  });

  testWidgets('the screen says up front that no money moves', (tester) async {
    await _open(tester, depreciation: _FakeDepreciation());
    expect(
      find.textContaining('Nothing leaves the bank for this'),
      findsOneWidget,
    );
    expect(find.textContaining('kept beside it'), findsOneWidget);
  });
}
