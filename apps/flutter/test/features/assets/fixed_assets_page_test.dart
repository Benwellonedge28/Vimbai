// The register on screen: what it says, and what it refuses to say.
//
// The two failures worth holding: a total that included a sold asset would overstate what the business owns, and a month
// charged twice would write the asset down twice. Both are checked here, along with the promise that a read that failed is
// never shown as an empty register.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_asset_register.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_assets_service.dart';
import 'package:vimbai_mobile_client/features/assets/presentation/fixed_asset_page.dart';
import 'package:vimbai_mobile_client/features/assets/presentation/fixed_assets_page.dart';

String _asset({
  String id = 'asset-1',
  String name = 'Delivery van',
  String cost = '4500.000000',
  String salvage = '500.000000',
  String accumulated = '0.000000',
  String netBookValue = '4500.000000',
  int life = 5,
  String status = 'active',
}) => jsonEncode({
  'id': id,
  'asset_code': 'VEH-001',
  'asset_name': name,
  'category': 'vehicles',
  'acquisition_date': '2026-01-15',
  'acquisition_cost': cost,
  'salvage_value': salvage,
  'accumulated_depreciation': accumulated,
  'net_book_value': netBookValue,
  'useful_life_years': life,
  'depreciation_method': 'straight_line',
  'status': status,
});

/// A service answering from canned engine answers, and everything it was asked.
class _Fake {
  _Fake({
    List<String>? register,
    String? one,
    List<String>? history,
    int registerStatus = 200,
    int chargeStatus = 200,
  }) : _register = register,
       _one = one,
       _history = history ?? const [],
       _registerStatus = registerStatus,
       _chargeStatus = chargeStatus;

  final List<String>? _register;
  final String? _one;
  final List<String> _history;
  final int _registerStatus;
  final int _chargeStatus;

  final List<http.Request> sent = [];

  late final FixedAssetsService service = FixedAssetsService.fresh()
    ..useClient(
      MockClient((request) async {
        sent.add(request);
        final path = request.url.path;
        if (path.endsWith('/depreciate')) {
          if (_chargeStatus != 200) {
            return http.Response(
              '{"detail":"Depreciation already recorded for period 2026-11"}',
              _chargeStatus,
            );
          }
          return http.Response(_one ?? _asset(), 200);
        }
        if (path.endsWith('/depreciation')) {
          return http.Response('[${_history.join(',')}]', 200);
        }
        if (path.endsWith('/assets')) {
          if (_registerStatus != 200) {
            return http.Response('{"detail":"no book"}', _registerStatus);
          }
          return http.Response('[${(_register ?? const []).join(',')}]', 200);
        }
        return http.Response(_one ?? _asset(), 200);
      }),
    );
}

Widget _wrapped(Widget page) => MaterialApp(home: page);

void main() {
  testWidgets('the register leads with what the held assets are worth', (
    tester,
  ) async {
    final fake = _Fake(
      register: [
        _asset(id: 'a', name: 'Delivery van', netBookValue: '4500.000000'),
        _asset(
          id: 'b',
          name: 'Old press',
          cost: '2000.000000',
          netBookValue: '2000.000000',
          status: 'disposed',
        ),
      ],
    );

    await tester.pumpWidget(
      _wrapped(FixedAssetsPage(service: fake.service, today: '2026-11-18')),
    );
    await tester.pumpAndSettle();

    // 4500, not 6500: a sold asset is history and cannot be counted as something owned
    expect(find.text('4500.00'), findsWidgets);
    expect(find.text('6500.00'), findsNothing);
    expect(find.textContaining('1 thing held'), findsOneWidget);
    expect(find.textContaining('sold or scrapped'), findsWidgets);
  });

  testWidgets(
    'an empty register says nothing is on it, not that it is worth nothing',
    (tester) async {
      final fake = _Fake(register: const []);

      await tester.pumpWidget(
        _wrapped(FixedAssetsPage(service: fake.service, today: '2026-11-18')),
      );
      await tester.pumpAndSettle();

      expect(find.text('Nothing is on the register yet'), findsOneWidget);
      expect(find.text('Worth now'), findsNothing);
    },
  );

  testWidgets('a read that failed is a sentence, never an empty register', (
    tester,
  ) async {
    final fake = _Fake(registerStatus: 500);

    await tester.pumpWidget(
      _wrapped(FixedAssetsPage(service: fake.service, today: '2026-11-18')),
    );
    await tester.pumpAndSettle();

    expect(find.byType(CouldNotRead), findsOneWidget);
    expect(find.text('Nothing is on the register yet'), findsNothing);
  });

  testWidgets('an asset with no life is named on the register', (tester) async {
    final fake = _Fake(register: [_asset(life: 0)]);

    await tester.pumpWidget(
      _wrapped(FixedAssetsPage(service: fake.service, today: '2026-11-18')),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('no useful life recorded'), findsOneWidget);
    expect(find.textContaining('Delivery van'), findsWidgets);
  });

  testWidgets('the form offers all three ways of writing an asset off', (
    tester,
  ) async {
    final fake = _Fake(register: const []);

    await tester.pumpWidget(
      _wrapped(FixedAssetsPage(service: fake.service, today: '2026-11-18')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byType(FloatingActionButton));
    await tester.pumpAndSettle();

    expect(find.text('Written off'), findsOneWidget);
    // the one the form starts on, and what it means
    expect(find.text('The same every month'), findsWidgets);
    expect(
      find.textContaining(
        'what it cost, less what it will be worth at the end',
      ),
      findsOneWidget,
    );

    await tester.tap(find.text('The same every month'));
    await tester.pumpAndSettle();
    // all three are on offer, in a person's words
    expect(find.text('A share of what is left'), findsWidgets);
    expect(find.text('For the time it was held'), findsWidgets);

    // a rate belongs to reducing balance alone, so it is only asked for once that is picked
    expect(find.text('Rate a year (optional)'), findsNothing);
    await tester.tap(find.text('A share of what is left').last);
    await tester.pumpAndSettle();
    expect(find.text('Rate a year (optional)'), findsOneWidget);
    expect(
      find.textContaining('Leave the rate empty to use its life'),
      findsOneWidget,
    );
  });

  testWidgets(
    'a time-based asset is not offered a charge for a month it was not held',
    (tester) async {
      final fake = _Fake(
        one: jsonEncode({
          'id': 'asset-1',
          'asset_code': 'VEH-001',
          'asset_name': 'Delivery van',
          'category': 'vehicles',
          // bought after the month the page is showing
          'acquisition_date': '2026-11-20',
          'acquisition_cost': '4500.000000',
          'salvage_value': '500.000000',
          'accumulated_depreciation': '0.000000',
          'net_book_value': '4500.000000',
          'useful_life_years': 5,
          'depreciation_method': 'time_based',
          'status': 'active',
        }),
      );

      await tester.pumpWidget(
        _wrapped(
          FixedAssetPage(
            service: fake.service,
            assetId: 'asset-1',
            month: '2026-10',
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.textContaining('It was not yours in 2026-10'),
        findsOneWidget,
      );
      // nothing is offered, so nothing can be charged by a tap
      final button = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Charge 0.00 for 2026-10'),
      );
      expect(button.onPressed, isNull);
    },
  );

  testWidgets('the asset page offers the month charge and says what is left', (
    tester,
  ) async {
    final fake = _Fake(one: _asset());

    await tester.pumpWidget(
      _wrapped(
        FixedAssetPage(
          assetId: 'asset-1',
          service: fake.service,
          month: '2026-11',
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('would take 66.67'), findsOneWidget);
    expect(find.text('Charge 66.67 for 2026-11'), findsOneWidget);
  });

  testWidgets('an asset says the rate it was registered with', (tester) async {
    final fake = _Fake(
      one: jsonEncode({
        'id': 'asset-1',
        'asset_code': 'PRS-001',
        'asset_name': 'Printing press',
        'category': 'machinery',
        'acquisition_date': '2026-01-15',
        'acquisition_cost': '240000.000000',
        'salvage_value': '40000.000000',
        'accumulated_depreciation': '0.000000',
        'net_book_value': '240000.000000',
        'useful_life_years': 10,
        'depreciation_method': 'reducing_balance',
        'depreciation_rate': '25.000000',
        'status': 'active',
      }),
    );

    await tester.pumpWidget(
      _wrapped(FixedAssetPage(service: fake.service, assetId: 'asset-1')),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('25% a year'), findsOneWidget);
    // 240000 * 25% / 12: the charge is the stated rate's, not the ten-year life's
    expect(find.textContaining('5000.00'), findsWidgets);
  });

  testWidgets('a month already charged cannot be charged again', (
    tester,
  ) async {
    final fake = _Fake(
      one: _asset(),
      history: [
        jsonEncode({
          'period': '2026-11',
          'depreciation_amount': '66.666667',
          'accumulated_depreciation': '66.666667',
          'net_book_value': '4433.333333',
        }),
      ],
    );

    await tester.pumpWidget(
      _wrapped(
        FixedAssetPage(
          assetId: 'asset-1',
          service: fake.service,
          month: '2026-11',
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('2026-11 has already been charged'),
      findsOneWidget,
    );
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, '2026-11 is charged'),
    );
    expect(
      button.onPressed,
      isNull,
      reason: 'a charged month must not be charged again',
    );
  });

  testWidgets('an asset already sold offers no charge at all', (tester) async {
    final fake = _Fake(one: _asset(status: 'disposed'));

    await tester.pumpWidget(
      _wrapped(
        FixedAssetPage(
          assetId: 'asset-1',
          service: fake.service,
          month: '2026-11',
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('sold or scrapped, so nothing more'),
      findsOneWidget,
    );
    expect(find.textContaining('Charge '), findsNothing);
  });

  testWidgets('the service refuses a cost of nothing before sending it', (
    tester,
  ) async {
    final fake = _Fake();
    var refused = '';
    try {
      await fake.service.buy(
        code: 'VEH-002',
        name: 'Second van',
        category: AssetCategory.equipment,
        cost: ExactAmount.zero,
        usefulLifeYears: 4,
        acquiredOn: '2026-11-18',
      );
    } catch (failure) {
      refused = CouldNotRead.reasonOf(failure);
    }

    expect(refused, contains('bought for nothing'));
    expect(fake.sent, isEmpty);
  });
}
