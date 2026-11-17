/// Reading money that the engine sends as a decimal string.
///
/// `vimbai_common::Money` serialises as a JSON *string* - `"250.000000"` - because an exact decimal does not survive a
/// double. A model field declared `double` with no converter casts that string to a number and throws, so every journal
/// entry and every trial-balance row failed to parse on the phone the moment the engine answered correctly: the screen
/// was not wrong about the arithmetic, it could not read the reply. This reads either form, so a number is still
/// accepted where one is sent.
///
/// This is for *display* only. Anything that decides whether an entry balances, or that writes an amount the ledger will
/// hold, uses [ExactAmount] instead: a double cannot add up money without losing it.
double amountFromJson(Object? value) {
  if (value == null) return 0.0;
  if (value is num) return value.toDouble();
  if (value is String) return double.tryParse(value.trim()) ?? 0.0;
  return 0.0;
}

/// An amount of money, held exactly.
///
/// The ledger counts in six decimal places and in as many whole digits as a figure happens to have - a thirty-digit
/// balance is a normal balance to `vimbai_common::Money`. A `double` holds fifteen significant digits in total, so the
/// phone could not even read such an amount back, let alone add two of them up and be sure the answer balanced. This
/// holds the digits themselves: an arbitrary-length integer of millionths, with the sign kept apart, and every sum, every
/// difference and every comparison done on those digits.
///
/// Six decimal places is the ledger's own unit - millionths - so the fraction is never rounded on the way in or on the way
/// out, and an amount entered with more precision than a ledger holds is refused rather than quietly trimmed.
class ExactAmount implements Comparable<ExactAmount> {
  /// Decimal places every amount is counted in. `vimbai_common::Money` has the same six.
  static const int scale = 6;

  /// True when the value is below zero. Absent sign means the digits alone say nothing about direction.
  final bool negative;

  /// The millionths, as digits: no leading zeros, no sign, `'0'` for zero.
  final String digits;

  const ExactAmount._(this.negative, this.digits);

  /// Nothing owed either way.
  static const ExactAmount zero = ExactAmount._(false, '0');

  /// An exact amount from a whole number of millionths.
  factory ExactAmount.micros(int micros) {
    if (micros == 0) return zero;
    final magnitude = micros < 0 ? -micros : micros;
    return ExactAmount._(micros < 0, _strip(magnitude.toString()));
  }

  /// An exact amount from whole units, for constants and tests.
  factory ExactAmount.units(int units) => ExactAmount.micros(units * 1000000);

  /// The value of [text], or null when it is not a plain decimal figure.
  ///
  /// Accepted: an optional sign, digits, and at most [scale] decimal places - `"25"`, `"-3.50"`, `"1234567890123.000001"`.
  /// Thousands separators, exponents and currency symbols are refused: a phone number typed into an amount field is not
  /// a figure to guess at.
  static ExactAmount? tryParse(String text) {
    final trimmed = text.trim();
    if (trimmed.isEmpty) return null;
    var body = trimmed;
    var negative = false;
    if (body.startsWith('-') || body.startsWith('+')) {
      negative = body.startsWith('-');
      body = body.substring(1);
    }
    if (body.isEmpty) return null;

    final dot = body.indexOf('.');
    final whole = dot < 0 ? body : body.substring(0, dot);
    final fraction = dot < 0 ? '' : body.substring(dot + 1);
    if (dot >= 0 && body.indexOf('.', dot + 1) >= 0) return null;
    if (!_isDigits(whole) && whole.isNotEmpty) return null;
    if (whole.isEmpty && fraction.isEmpty) return null;
    if (!_isDigits(fraction)) return null;
    // More decimal places than a ledger holds is a typo or a wish, not an amount: refuse rather than round it.
    if (fraction.length > scale) {
      if (fraction.substring(scale).split('').any((d) => d != '0')) return null;
      return ExactAmount._(
        negative,
        _strip(whole.isEmpty ? '0' : whole) + fraction.substring(0, scale),
      )._normalised();
    }
    final padded = fraction.padRight(scale, '0');
    final unscaled = _strip((whole.isEmpty ? '0' : whole) + padded);
    return ExactAmount._(negative && unscaled != '0', unscaled);
  }

  /// Parses [text], or throws with the text in the message so a form can say what it could not read.
  factory ExactAmount.parse(String text) {
    final parsed = tryParse(text);
    if (parsed == null) {
      throw FormatException('Not an amount: "$text"');
    }
    return parsed;
  }

  /// Reads an amount from whatever the engine or the local store sends: a decimal string, a whole number, or nothing.
  ///
  /// A `double` that has already lost digits is read for what it says rather than refused - refusing would blank a
  /// screen over a value the phone itself never should have stored.
  factory ExactAmount.fromJson(Object? value) {
    if (value == null) return zero;
    if (value is String) return tryParse(value) ?? zero;
    if (value is int) return ExactAmount.micros(value * 1000000);
    if (value is double) {
      if (value.isNaN || value.isInfinite) return zero;
      return tryParse(value.toString()) ?? zero;
    }
    return zero;
  }

  /// This amount as a plain decimal figure, the way a person writes money.
  @override
  String toString() => toDecimalString();

  /// The value with at least two decimal places and never more than [scale], trailing zeros trimmed beyond the second.
  String toDecimalString() {
    final sign = negative ? '-' : '';
    if (digits.length <= scale) {
      final padded = digits.padLeft(scale + 1, '0');
      final whole = padded.substring(0, padded.length - scale);
      final fraction = padded.substring(padded.length - scale);
      return '$sign$whole.${_trimmed(fraction)}';
    }
    final cut = digits.length - scale;
    final fraction = _trimmed(digits.substring(cut));
    return '$sign${digits.substring(0, cut)}.$fraction';
  }

  /// Six decimal places exactly, which is the shape the ledger stores.
  String toLedgerString() {
    final sign = negative ? '-' : '';
    final padded = digits.padLeft(scale + 1, '0');
    final cut = padded.length - scale;
    return '$sign${padded.substring(0, cut)}.${padded.substring(cut)}';
  }

  ExactAmount operator +(ExactAmount other) {
    if (negative == other.negative) {
      return ExactAmount._(
        negative,
        _strip(_addDigits(digits, other.digits)),
      )._normalised();
    }
    final comparison = _compareDigits(digits, other.digits);
    if (comparison == 0) return zero;
    if (comparison > 0) {
      return ExactAmount._(
        negative,
        _strip(_subtractDigits(digits, other.digits)),
      )._normalised();
    }
    return ExactAmount._(
      other.negative,
      _strip(_subtractDigits(other.digits, digits)),
    )._normalised();
  }

  ExactAmount operator -(ExactAmount other) => this + other.negated();

  /// The same amount in the other direction.
  ExactAmount negated() => isZero ? zero : ExactAmount._(!negative, digits);

  /// True when this amount is exactly nothing.
  bool get isZero => digits == '0';

  /// True when this amount is below zero.
  bool get isNegative => negative && digits != '0';

  @override
  int compareTo(ExactAmount other) {
    if (negative != other.negative) {
      if (digits == '0' && other.digits == '0') return 0;
      return negative ? -1 : 1;
    }
    final magnitude = _compareDigits(digits, other.digits);
    return negative ? -magnitude : magnitude;
  }

  bool operator >(ExactAmount other) => compareTo(other) > 0;
  bool operator <(ExactAmount other) => compareTo(other) < 0;
  bool operator >=(ExactAmount other) => compareTo(other) >= 0;
  bool operator <=(ExactAmount other) => compareTo(other) <= 0;

  @override
  bool operator ==(Object other) =>
      other is ExactAmount &&
      other.negative == negative &&
      other.digits == digits;

  @override
  int get hashCode => Object.hash(negative, digits);

  /// The sum of [amounts], from nothing.
  static ExactAmount sum(Iterable<ExactAmount> amounts) =>
      amounts.fold(zero, (total, amount) => total + amount);

  /// Keeps the sign only where the digits are not zero.
  ExactAmount _normalised() =>
      digits == '0' ? zero : ExactAmount._(negative, digits);

  static bool _isDigits(String text) =>
      text.isEmpty || text.codeUnits.every((c) => c >= 0x30 && c <= 0x39);

  static String _strip(String digits) {
    var start = 0;
    while (start < digits.length - 1 && digits.codeUnitAt(start) == 0x30) {
      start++;
    }
    final trimmed = digits.substring(start);
    return trimmed.isEmpty ? '0' : trimmed;
  }

  static String _trimmed(String fraction) {
    var end = fraction.length;
    while (end > 2 && fraction.codeUnitAt(end - 1) == 0x30) {
      end--;
    }
    return fraction.substring(0, end);
  }

  static int _compareDigits(String a, String b) {
    if (a.length != b.length) return a.length < b.length ? -1 : 1;
    final order = a.compareTo(b);
    return order == 0 ? 0 : (order < 0 ? -1 : 1);
  }

  static String _addDigits(String a, String b) {
    final out = StringBuffer();
    var carry = 0;
    var i = a.length - 1;
    var j = b.length - 1;
    while (i >= 0 || j >= 0 || carry != 0) {
      final left = i >= 0 ? a.codeUnitAt(i) - 0x30 : 0;
      final right = j >= 0 ? b.codeUnitAt(j) - 0x30 : 0;
      final total = left + right + carry;
      out.writeCharCode(0x30 + total % 10);
      carry = total ~/ 10;
      i--;
      j--;
    }
    return out.toString().split('').reversed.join();
  }

  /// [a] minus [b] where a is the larger of the two.
  static String _subtractDigits(String a, String b) {
    final out = StringBuffer();
    var borrow = 0;
    var i = a.length - 1;
    var j = b.length - 1;
    while (i >= 0) {
      final left = a.codeUnitAt(i) - 0x30 - borrow;
      final right = j >= 0 ? b.codeUnitAt(j) - 0x30 : 0;
      var difference = left - right;
      if (difference < 0) {
        difference += 10;
        borrow = 1;
      } else {
        borrow = 0;
      }
      out.writeCharCode(0x30 + difference);
      i--;
      j--;
    }
    return out.toString().split('').reversed.join();
  }
}

/// Reads an amount from JSON that the engine sent as a decimal string.
ExactAmount exactAmountFromJson(Object? value) => ExactAmount.fromJson(value);

/// Writes an amount as the decimal string the ledger stores, so no digit is lost on the way out.
String exactAmountToJson(ExactAmount amount) => amount.toLedgerString();
