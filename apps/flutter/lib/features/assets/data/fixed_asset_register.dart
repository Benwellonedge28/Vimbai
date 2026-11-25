// The things a business owns and uses, and what they are worth now.
//
// A fixed asset is bought once and used for years, so the register answers two questions and never confuses them: what it
// cost, and what is left of that cost. The engine keeps both - accumulated depreciation and net book value - and this file
// reads them rather than working them out, because a phone that recalculated depreciation would eventually disagree with
// the ledger it is reporting on.
//
// The one thing that is counted here is what the register as a whole comes to, and only from the rows in front of it, so a
// total on screen can never include an asset the list did not show.
import 'package:vimbai_mobile_client/core/api/amount.dart';

/// What a category of asset is called on the wire, and in a person's words.
enum AssetCategory {
  buildings('buildings', 'Buildings'),
  machinery('machinery', 'Machinery'),
  equipment('equipment', 'Equipment'),
  vehicles('vehicles', 'Vehicles'),
  furniture('furniture', 'Furniture'),
  computers('computers', 'Computers'),
  land('land', 'Land'),
  other('other', 'Other');

  const AssetCategory(this.wire, this.label);

  final String wire;
  final String label;

  /// The category the engine recorded, or [other] for one this app does not offer.
  static AssetCategory fromWire(String? wire) {
    for (final category in AssetCategory.values) {
      if (category.wire == wire) return category;
    }
    return AssetCategory.other;
  }
}

/// How the engine writes an asset's cost off over its life.
///
/// One is chosen per asset, so a business can write a building down the same every month and a van by a share of what is
/// left, in the same register. The engine refuses any other name rather than charging nothing.
enum DepreciationMethod {
  straightLine('straight_line', 'The same every month'),
  reducingBalance('reducing_balance', 'A share of what is left'),
  timeBased('time_based', 'For the time it was held');

  const DepreciationMethod(this.wire, this.label);

  final String wire;
  final String label;

  static DepreciationMethod fromWire(String? wire) {
    for (final method in DepreciationMethod.values) {
      if (method.wire == wire) return method;
    }
    return DepreciationMethod.straightLine;
  }
}

/// One thing the business owns.
class FixedAsset {
  final String id;
  final String code;
  final String name;
  final AssetCategory category;
  final String location;
  final String department;
  final String acquiredOn;
  final ExactAmount cost;
  final ExactAmount salvageValue;
  final ExactAmount accumulatedDepreciation;
  final ExactAmount netBookValue;
  final int usefulLifeYears;
  final DepreciationMethod method;

  /// The share of what is left a reducing-balance asset gives up in a year, as a percentage.
  ///
  /// Zero when the asset states none, and the engine then charges the life's own share. Kept rather than assumed,
  /// because a machine written down at 25% a year is not the same figure as its life would give.
  final ExactAmount rate;
  final String status;

  const FixedAsset({
    required this.id,
    required this.code,
    required this.name,
    required this.category,
    required this.location,
    required this.department,
    required this.acquiredOn,
    required this.cost,
    required this.salvageValue,
    required this.accumulatedDepreciation,
    required this.netBookValue,
    required this.usefulLifeYears,
    required this.method,
    this.rate = ExactAmount.zero,
    required this.status,
  });

  factory FixedAsset.fromJson(Map<String, dynamic> json) => FixedAsset(
    id: (json['id'] as String?) ?? '',
    code: (json['asset_code'] as String?) ?? '',
    name: (json['asset_name'] as String?) ?? '',
    category: AssetCategory.fromWire(json['category'] as String?),
    location: (json['location'] as String?) ?? '',
    department: (json['department'] as String?) ?? '',
    acquiredOn: (json['acquisition_date'] as String?) ?? '',
    cost: ExactAmount.fromJson(json['acquisition_cost']),
    salvageValue: ExactAmount.fromJson(json['salvage_value']),
    accumulatedDepreciation: ExactAmount.fromJson(
      json['accumulated_depreciation'],
    ),
    netBookValue: ExactAmount.fromJson(json['net_book_value']),
    usefulLifeYears: (json['useful_life_years'] as num?)?.toInt() ?? 0,
    method: DepreciationMethod.fromWire(json['depreciation_method'] as String?),
    rate: ExactAmount.fromJson(json['depreciation_rate']),
    status: (json['status'] as String?) ?? 'active',
  );

  /// True while the asset is still in use. A disposed asset keeps its row: it happened, and the register is a record.
  bool get isHeld => status == 'active';

  /// How much of the cost has been written off so far.
  ExactAmount get writtenOff => accumulatedDepreciation;

  /// True when the engine would charge nothing this month.
  ///
  /// Straight line with no useful life divides by zero months and answers zero, so an asset registered without a life is
  /// carried at cost for ever. That is a mistake worth naming rather than a quiet zero in a column.
  bool get chargesNothing => isHeld && usefulLifeYears <= 0;

  /// True when nothing is left to write off: the asset is down to what it would fetch at the end.
  bool get fullyWrittenDown => netBookValue <= salvageValue;

  /// What the engine would charge this asset for [month], in the method its register entry names.
  ///
  /// This is the engine's own rule repeated, not a second opinion: straight line spreads cost less salvage over the
  /// months of its life, reducing balance takes the asset's stated rate of what is left (or the life's own share when it
  /// states none), and time based charges the annual figure for the days of the month the asset was held. Every charge is
  /// rounded to the cent, which is what the engine charges - proved live: a van bought for 4500.00 with 500.00 at the end
  /// over five years is charged 66.67, not 66.666667. A figure promised here that the engine then did not take would be
  /// the page lying about the next balance.
  ///
  /// Nothing is charged past what is left to write off, so the last month of an asset's life is charged the remainder,
  /// exactly as the register charges it.
  ExactAmount chargeFor(String month) {
    if (!isHeld) return ExactAmount.zero;
    final charge = switch (method) {
      DepreciationMethod.straightLine =>
        usefulLifeYears <= 0
            ? ExactAmount.zero
            : _divide(cost - salvageValue, usefulLifeYears * 12),
      DepreciationMethod.reducingBalance => _divide(
        netBookValue * _annualPercent,
        1200,
      ),
      DepreciationMethod.timeBased => _chargeByTime(month),
    };
    final remaining = cost - salvageValue - accumulatedDepreciation;
    if (remaining <= ExactAmount.zero) return ExactAmount.zero;
    return charge > remaining ? remaining : charge;
  }

  /// The share of what is left a reducing-balance asset gives up in a year, as a percentage.
  ExactAmount get _annualPercent {
    if (!rate.isZero) return rate;
    if (usefulLifeYears <= 0) return ExactAmount.zero;
    return _share(ExactAmount.parse('100'), usefulLifeYears);
  }

  /// What a time-based asset is charged for [month]: the annual figure, for the days of it the asset was held.
  ExactAmount _chargeByTime(String month) {
    final days = daysHeldIn(month);
    if (days <= 0) return ExactAmount.zero;
    final annual = _share(cost - salvageValue, usefulLifeYears);
    return _portion(annual, days, 365);
  }

  /// The days of [month] the asset was held, counting the day it arrived and the day its life ends.
  ///
  /// Zero when the month lies outside the asset's life altogether, which the engine refuses in words rather than charging
  /// nothing: a charge of nothing in a column looks like a month the asset stood still.
  int daysHeldIn(String month) {
    final start = _monthStart(month);
    final acquired = _acquiredDay;
    if (start == null || acquired == null || usefulLifeYears <= 0) return 0;
    final end = DateTime.utc(start.year, start.month + 1, 0);
    final lifeEnds = acquired.add(Duration(days: usefulLifeYears * 365));
    final from = acquired.isAfter(start) ? acquired : start;
    final to = lifeEnds.isBefore(end) ? lifeEnds : end;
    if (to.isBefore(from)) return 0;
    return to.difference(from).inDays + 1;
  }

  /// Why the engine would refuse to charge [month], in a person's words, or null when it would charge it.
  String? refusalFor(String month) {
    if (method != DepreciationMethod.timeBased || !isHeld) return null;
    if (daysHeldIn(month) > 0) return null;
    final acquired = _acquiredDay;
    if (acquired == null) {
      return 'This one was registered without a date it arrived, so the time '
          'it has been held cannot be counted.';
    }
    if (usefulLifeYears <= 0) {
      return 'No useful life was recorded, so a charge by time has no rate to '
          'work from.';
    }
    final start = _monthStart(month);
    if (start != null &&
        start.isBefore(DateTime.utc(acquired.year, acquired.month, 1))) {
      return 'It was not yours in $month: it arrived on ${acquiredOn.trim()}.';
    }
    final lifeEnds = acquired.add(Duration(days: usefulLifeYears * 365));
    return 'Its $usefulLifeYears-year life ended on '
        '${lifeEnds.year.toString().padLeft(4, '0')}-'
        '${lifeEnds.month.toString().padLeft(2, '0')}-'
        '${lifeEnds.day.toString().padLeft(2, '0')}, so nothing is charged '
        'for $month.';
  }

  /// The first day of a `YYYY-MM` month, or null when the text is not one.
  static DateTime? _monthStart(String month) {
    final parts = month.split('-');
    if (parts.length != 2) return null;
    final year = int.tryParse(parts[0]);
    final number = int.tryParse(parts[1]);
    if (year == null || number == null || number < 1 || number > 12) {
      return null;
    }
    return DateTime.utc(year, number, 1);
  }

  /// The day the asset arrived, as a calendar day rather than an instant.
  DateTime? get _acquiredDay {
    final text = acquiredOn.trim();
    if (text.length < 10) return null;
    final parsed = DateTime.tryParse(text.substring(0, 10));
    if (parsed == null) return null;
    return DateTime.utc(parsed.year, parsed.month, parsed.day);
  }

  /// The month after [month], counting months rather than days.
  static String nextPeriod(String month) => shiftPeriod(month, 1);

  /// [month] moved by [months], in `YYYY-MM` form.
  static String shiftPeriod(String month, int months) {
    final parts = month.split('-');
    if (parts.length != 2) return month;
    final year = int.tryParse(parts[0]);
    final monthNumber = int.tryParse(parts[1]);
    if (year == null || monthNumber == null) return month;
    final moved = (year * 12 + (monthNumber - 1)) + months;
    final newYear = moved ~/ 12;
    final newMonth = moved % 12 + 1;
    return '${newYear.toString().padLeft(4, '0')}-'
        '${newMonth.toString().padLeft(2, '0')}';
  }
}

/// One month's depreciation as the engine recorded it.
class DepreciationCharge {
  final String period;
  final ExactAmount amount;
  final ExactAmount accumulated;
  final ExactAmount netBookValue;

  const DepreciationCharge({
    required this.period,
    required this.amount,
    required this.accumulated,
    required this.netBookValue,
  });

  factory DepreciationCharge.fromJson(Map<String, dynamic> json) =>
      DepreciationCharge(
        period: (json['period'] as String?) ?? '',
        amount: ExactAmount.fromJson(json['depreciation_amount']),
        accumulated: ExactAmount.fromJson(json['accumulated_depreciation']),
        netBookValue: ExactAmount.fromJson(json['net_book_value']),
      );
}

/// What the register as a whole comes to, counted from the rows on the list.
class AssetRegister {
  final List<FixedAsset> assets;

  const AssetRegister(this.assets);

  /// The register as the engine listed it: biggest remaining value first, because that is the one worth watching.
  factory AssetRegister.fromJson(Object? answer) {
    final rows = [
      for (final row in (answer as List?) ?? const [])
        FixedAsset.fromJson(row as Map<String, dynamic>),
    ];
    rows.sort((a, b) {
      final byValue = b.netBookValue.compareTo(a.netBookValue);
      if (byValue != 0) return byValue;
      return a.name.toLowerCase().compareTo(b.name.toLowerCase());
    });
    return AssetRegister(rows);
  }

  bool get isEmpty => assets.isEmpty;

  /// Only the things still held: a disposed asset is history, and it is not worth anything to the business.
  List<FixedAsset> get held => [
    for (final asset in assets)
      if (asset.isHeld) asset,
  ];

  /// What everything still held is worth now.
  ExactAmount get worthNow => _sum(held.map((asset) => asset.netBookValue));

  /// What everything still held cost.
  ExactAmount get costOfWhatIsHeld => _sum(held.map((asset) => asset.cost));

  /// How much of that cost has been written off.
  ExactAmount get writtenOff => _sum(held.map((asset) => asset.writtenOff));

  /// What the business would still have to write off if nothing were bought or sold: what the held assets are worth now,
  /// less what they are expected to be worth at the end.
  ExactAmount get stillToWriteOff =>
      worthNow - _sum(held.map((asset) => asset.salvageValue));

  /// The assets whose charge this month would be nothing, and which need a useful life first.
  List<FixedAsset> get withoutALife => [
    for (final asset in held)
      if (asset.chargesNothing) asset,
  ];

  /// What each category is worth now, largest first, leaving out categories worth nothing.
  List<AssetCategoryTotal> get byCategory {
    final totals = <AssetCategory, (int, ExactAmount)>{};
    for (final asset in held) {
      final seen = totals[asset.category];
      totals[asset.category] = seen == null
          ? (1, asset.netBookValue)
          : (seen.$1 + 1, seen.$2 + asset.netBookValue);
    }
    final rows = [
      for (final entry in totals.entries)
        if (entry.value.$2 > ExactAmount.zero || entry.value.$1 > 0)
          AssetCategoryTotal(
            category: entry.key,
            count: entry.value.$1,
            worth: entry.value.$2,
          ),
    ];
    rows.sort((a, b) {
      final byWorth = b.worth.compareTo(a.worth);
      if (byWorth != 0) return byWorth;
      return a.category.label.compareTo(b.category.label);
    });
    return rows;
  }

  /// One asset from this register, by its id.
  FixedAsset? byId(String id) {
    for (final asset in assets) {
      if (asset.id == id) return asset;
    }
    return null;
  }

  static ExactAmount _sum(Iterable<ExactAmount> amounts) =>
      amounts.fold(ExactAmount.zero, (running, amount) => running + amount);
}

/// One category, and what the business holds in it.
class AssetCategoryTotal {
  final AssetCategory category;
  final int count;
  final ExactAmount worth;

  const AssetCategoryTotal({
    required this.category,
    required this.count,
    required this.worth,
  });
}

/// Divides an exact amount by a whole number of parts, rounded to the cent.
///
/// ExactAmount has no division, because a division that loses digits is exactly what this app refuses to do silently. A
/// depreciation charge is the one place the engine itself rounds: it keeps the charge to two places, so a phone that
/// promised six would promise a figure the register never takes. The rounding happens in whole cents here, which is
/// exact for any amount a business can hold.
/// [amount] over [parts], kept to the millionth the register works in.
///
/// The engine works an annual figure out once and then takes a part of it, so the part is taken of the same figure
/// rather than of a rounded one: a time-based charge divided twice would drift a cent away from what the register takes.
ExactAmount _share(ExactAmount amount, int parts) {
  if (parts == 0) return ExactAmount.zero;
  final micros = int.tryParse(amount.digits);
  if (micros == null) return ExactAmount.zero;
  final rounded = (micros + parts ~/ 2) ~/ parts;
  return ExactAmount.micros(amount.isNegative ? -rounded : rounded);
}

/// [amount] for [days] of a [daysInYear] year, rounded to the cent as the engine rounds it.
ExactAmount _portion(ExactAmount amount, int days, int daysInYear) {
  if (daysInYear == 0) return ExactAmount.zero;
  final micros = int.tryParse(amount.digits);
  if (micros == null) return ExactAmount.zero;
  const int perCent = 10000;
  final divisor = daysInYear * perCent;
  final cents = (micros * days + divisor ~/ 2) ~/ divisor;
  final rounded = cents * perCent;
  return ExactAmount.micros(amount.isNegative ? -rounded : rounded);
}

ExactAmount _divide(ExactAmount amount, int parts) {
  if (parts == 0) return ExactAmount.zero;
  final micros = int.tryParse(amount.digits);
  if (micros == null) return ExactAmount.zero;
  // one cent is 10,000 millionths at this scale
  const int perCent = 10000;
  final cents = (micros + parts * perCent ~/ 2) ~/ (parts * perCent);
  final rounded = cents * perCent;
  return ExactAmount.micros(amount.isNegative ? -rounded : rounded);
}
