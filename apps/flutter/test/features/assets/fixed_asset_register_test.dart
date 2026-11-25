// The register of things a business owns, held to the figures the engine keeps.
//
// The value of the register is not a number this file invents: it is the sum of what each held asset is worth, so an asset
// that has been sold cannot quietly stay in the total. The month's charge is the engine's own rule repeated, and the tests
// below pin the two methods to the figures the engine answers with, so a disagreement shows up here rather than in a
// balance.
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_asset_register.dart';

Map<String, dynamic> _row({
  String id = 'asset-1',
  String name = 'Delivery van',
  String code = 'VEH-001',
  String category = 'vehicles',
  String cost = '4500.000000',
  String salvage = '500.000000',
  String accumulated = '0.000000',
  String netBookValue = '4500.000000',
  int life = 5,
  String method = 'straight_line',
  String? rate,
  String status = 'active',
  String acquired = '2026-01-15',
}) => {
  'id': id,
  'asset_code': code,
  'asset_name': name,
  'category': category,
  'acquisition_date': acquired,
  'acquisition_cost': cost,
  'salvage_value': salvage,
  'accumulated_depreciation': accumulated,
  'net_book_value': netBookValue,
  'useful_life_years': life,
  'depreciation_method': method,
  if (rate != null) 'depreciation_rate': rate,
  'status': status,
};

void main() {
  group('what the register comes to', () {
    test('it is the worth of what is held, not of everything ever bought', () {
      final register = AssetRegister.fromJson([
        _row(id: 'a', name: 'Van', netBookValue: '4500.000000'),
        _row(
          id: 'b',
          name: 'Old press',
          netBookValue: '0.000000',
          status: 'disposed',
        ),
      ]);

      expect(register.assets.length, 2);
      expect(register.held.length, 1);
      expect(register.worthNow.toDecimalString(), '4500.00');
      expect(register.costOfWhatIsHeld.toDecimalString(), '4500.00');
    });

    test('the write-off so far is what has come off the cost', () {
      final register = AssetRegister.fromJson([
        _row(accumulated: '1200.000000', netBookValue: '3300.000000'),
      ]);

      // 1200 has come off a 4500 cost, and 3300 is left: of that, 500 is what it will be worth at the end, so 2800 is
      // still to come off
      expect(register.writtenOff.toDecimalString(), '1200.00');
      expect(register.stillToWriteOff.toDecimalString(), '2800.00');
    });

    test('a long register keeps every digit, so a big business is exact', () {
      final register = AssetRegister.fromJson([
        _row(cost: '123456789012.123456', netBookValue: '123456789012.123456'),
        _row(id: 'b', cost: '0.000001', netBookValue: '0.000001'),
      ]);

      expect(register.worthNow.toDecimalString(), '123456789012.123457');
    });

    test('the biggest remaining value comes first', () {
      final register = AssetRegister.fromJson([
        _row(id: 'a', name: 'Chair', netBookValue: '50.000000'),
        _row(id: 'b', name: 'Van', netBookValue: '4500.000000'),
      ]);

      expect(register.assets.first.name, 'Van');
      expect(register.assets.last.name, 'Chair');
    });

    test(
      'an asset with no life is named rather than silently worth nothing',
      () {
        final register = AssetRegister.fromJson([
          _row(id: 'a', life: 0),
          _row(id: 'b', name: 'Press', life: 4),
        ]);

        expect(register.withoutALife.map((asset) => asset.id), ['a']);
      },
    );

    test(
      'categories are worth what they hold, largest first, and skip nothing',
      () {
        final register = AssetRegister.fromJson([
          _row(id: 'a', category: 'vehicles', netBookValue: '3000.000000'),
          _row(id: 'b', category: 'computers', netBookValue: '800.000000'),
          _row(id: 'c', category: 'computers', netBookValue: '200.000000'),
          _row(
            id: 'd',
            category: 'vehicles',
            netBookValue: '900.000000',
            status: 'disposed',
          ),
        ]);

        expect(register.byCategory.length, 2);
        expect(register.byCategory.first.category, AssetCategory.vehicles);
        expect(register.byCategory.first.worth.toDecimalString(), '3000.00');
        expect(register.byCategory.last.category, AssetCategory.computers);
        expect(register.byCategory.last.count, 2);
        expect(register.byCategory.last.worth.toDecimalString(), '1000.00');
      },
    );

    test(
      'an empty register says so rather than reading as a value of nothing',
      () {
        final register = AssetRegister.fromJson(const []);
        expect(register.isEmpty, isTrue);
        expect(register.worthNow.toDecimalString(), '0.00');
      },
    );

    test('a category this app does not offer still reads as something', () {
      final asset = FixedAsset.fromJson(_row(category: 'livestock'));
      expect(asset.category, AssetCategory.other);
    });
  });

  group('the month charge', () {
    test('straight line spreads cost less salvage over the months of its life', () {
      // 4500 - 500 over 5 years is 4000 over 60 months, which the engine rounds to the cent it actually charges: 66.67
      // (proved live against the register, which answered accumulated_depreciation 66.67 after the first month)
      final asset = FixedAsset.fromJson(_row());
      expect(asset.chargeFor('2026-11').toDecimalString(), '66.67');
    });

    test('reducing balance takes a twentieth of what is left', () {
      // five years is the life's own share: 100 / 5 is the twentieth the engine charges
      final asset = FixedAsset.fromJson(
        _row(method: 'reducing_balance', netBookValue: '2400.000000'),
      );
      expect(asset.chargeFor('2026-11').toDecimalString(), '40.00');
    });

    test(
      'an asset with no life charges nothing, and says it charges nothing',
      () {
        final asset = FixedAsset.fromJson(_row(life: 0));
        expect(asset.chargeFor('2026-11').isZero, isTrue);
        expect(asset.chargesNothing, isTrue);
      },
    );

    test('something already sold charges nothing, whatever its life says', () {
      final asset = FixedAsset.fromJson(_row(status: 'disposed'));
      expect(asset.chargeFor('2026-11').isZero, isTrue);
      expect(asset.chargesNothing, isFalse);
    });

    test('an asset down to its end value charges nothing more', () {
      final asset = FixedAsset.fromJson(
        _row(accumulated: '4000.000000', netBookValue: '500.000000'),
      );
      expect(asset.fullyWrittenDown, isTrue);
    });

    test('the last charge is what is left, not a whole month of it', () {
      // 4000 of depreciable cost, 3999.80 already written off: the engine takes the twenty cents, not 66.67
      final asset = FixedAsset.fromJson(
        _row(accumulated: '3999.800000', netBookValue: '500.200000'),
      );
      expect(asset.chargeFor('2026-11').toDecimalString(), '0.20');
    });

    test('reducing balance takes the rate the asset states', () {
      final asset = FixedAsset.fromJson(
        _row(
          method: 'reducing_balance',
          rate: '25.000000',
          netBookValue: '2400.000000',
        ),
      );
      // 2400 * 25% / 12
      expect(asset.chargeFor('2026-11').toDecimalString(), '50.00');
    });

    test(
      'reducing balance over a ten-year life is a tenth a year, not a fifth',
      () {
        final asset = FixedAsset.fromJson(
          _row(
            method: 'reducing_balance',
            life: 10,
            netBookValue: '2400.000000',
          ),
        );
        expect(asset.chargeFor('2026-11').toDecimalString(), '20.00');
      },
    );

    test('time based charges the days of the month the asset was held', () {
      // bought on the 15th of January: 4000 over five years is 800 a year, taken for the seventeen days from the 15th
      // to the 31st: 800 * 17 / 365
      final asset = FixedAsset.fromJson(
        _row(method: 'time_based', acquired: '2026-01-15'),
      );
      expect(asset.chargeFor('2026-01').toDecimalString(), '37.26');
      // a whole month is charged for a whole month: 800 * 30 / 365
      expect(asset.chargeFor('2026-11').toDecimalString(), '65.75');
      // and a 31-day month takes one day more than a 30-day one
      expect(asset.chargeFor('2026-12').toDecimalString(), '67.95');
    });

    test(
      'a month before the asset arrived is refused in words, not charged',
      () {
        final asset = FixedAsset.fromJson(
          _row(method: 'time_based', acquired: '2026-01-15'),
        );
        expect(asset.chargeFor('2025-12').isZero, isTrue);
        expect(asset.daysHeldIn('2025-12'), 0);
        expect(
          asset.refusalFor('2025-12'),
          'It was not yours in 2025-12: it arrived on 2026-01-15.',
        );
        expect(asset.refusalFor('2026-01'), isNull);
      },
    );

    test('a month after the life has run out is refused in words', () {
      final asset = FixedAsset.fromJson(
        _row(method: 'time_based', acquired: '2020-01-15', life: 5),
      );
      expect(asset.chargeFor('2026-11').isZero, isTrue);
      expect(asset.refusalFor('2026-11'), contains('life ended on 2025-01-13'));
    });

    test('the other two methods are never refused, whatever the month', () {
      // straight line and reducing balance have no calendar in them: a month is a month, whenever it falls
      final asset = FixedAsset.fromJson(_row());
      expect(asset.refusalFor('2030-01'), isNull);
      expect(asset.chargeFor('2030-01').toDecimalString(), '66.67');
    });
  });

  group('months', () {
    test('the next month rolls the year over', () {
      expect(FixedAsset.nextPeriod('2026-12'), '2027-01');
    });

    test('a month moved backwards crosses the year too', () {
      expect(FixedAsset.shiftPeriod('2026-01', -1), '2025-12');
      expect(FixedAsset.shiftPeriod('2026-03', -3), '2025-12');
    });

    test(
      'something that is not a month is left alone rather than guessed at',
      () {
        expect(FixedAsset.nextPeriod('November'), 'November');
      },
    );
  });

  test('a charge read back keeps the period it was charged for', () {
    final charge = DepreciationCharge.fromJson({
      'period': '2026-11',
      'depreciation_amount': '66.666667',
      'accumulated_depreciation': '733.333334',
      'net_book_value': '3766.666666',
    });
    expect(charge.period, '2026-11');
    expect(charge.amount.toDecimalString(), '66.666667');
    expect(charge.netBookValue.toDecimalString(), '3766.666666');
  });

  test('a figure the engine sent as a number is read the same as text', () {
    final asset = FixedAsset.fromJson({
      ..._row(),
      'acquisition_cost': 4500,
      'net_book_value': 4500.5,
    });
    expect(asset.cost, ExactAmount.parse('4500'));
    expect(asset.netBookValue.toDecimalString(), '4500.50');
  });
}
