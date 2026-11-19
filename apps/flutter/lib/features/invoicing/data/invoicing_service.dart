// Invoices and the customers they bill.
//
// The engine has kept both since the invoicing domain was ported - a Book's customers, the invoices raised against them,
// the lines an invoice is made of, and the five states it can be in - but the phone had no way to see or raise one, so a
// business could post entries all day and never bill anybody. This is that side of it.
//
// Amounts travel as exact decimal text, six places, never as a double: the engine reads a line's amount and refuses a
// total that does not match its lines to the cent, so an amount that went through a float could be the one thing that
// makes a correct invoice unraisable.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// A customer a Book bills.
class InvoiceCustomer {
  final String id;
  final String customerId;
  final String name;

  /// How to reach them. Kept by the engine since the domain was ported and empty until somebody fills them in.
  final String email;
  final String phone;
  final String address;

  const InvoiceCustomer({
    required this.id,
    required this.customerId,
    required this.name,
    this.email = '',
    this.phone = '',
    this.address = '',
  });

  factory InvoiceCustomer.fromJson(Map<String, dynamic> json) =>
      InvoiceCustomer(
        id: (json['id'] as String?) ?? '',
        customerId: (json['customer_id'] as String?) ?? '',
        name: (json['name'] as String?) ?? '',
        email: (json['email'] as String?) ?? '',
        phone: (json['phone'] as String?) ?? '',
        address: (json['address'] as String?) ?? '',
      );

  /// What a list shows when a customer was saved without a name.
  String get label => name.isNotEmpty ? name : customerId;

  /// Whether there is a way to reach them on file at all.
  bool get reachable => email.isNotEmpty || phone.isNotEmpty;
}

/// One line of an invoice: what was sold, how much of it, and at what price.
class InvoiceLine {
  final String description;
  final ExactAmount quantity;
  final ExactAmount unitPrice;

  const InvoiceLine({
    required this.description,
    required this.quantity,
    required this.unitPrice,
  });

  /// The line's own amount: quantity times unit price, exactly.
  ExactAmount get amount => quantity * unitPrice;

  Map<String, dynamic> toJson() => {
    'description': description,
    'quantity': quantity.toLedgerString(),
    'unit_price': unitPrice.toLedgerString(),
    'amount': amount.toLedgerString(),
  };
}

/// A raised invoice, as the engine holds it.
class Invoice {
  final String invoiceNumber;
  final String customerId;
  final String invoiceDate;
  final String dueDate;
  final ExactAmount total;

  /// Money taken off what the lines came to, given before the invoice was raised.
  final ExactAmount discount;

  /// What the customer owes once the discount is off, as the engine states it. Null on an invoice read from an older
  /// record, where [due] works it out from the total instead.
  final ExactAmount? amountDue;

  final ExactAmount paid;
  final String status;
  final List<InvoiceLine> lines;

  const Invoice({
    required this.invoiceNumber,
    required this.customerId,
    required this.invoiceDate,
    required this.dueDate,
    required this.total,
    this.discount = ExactAmount.zero,
    this.amountDue,
    required this.paid,
    required this.status,
    required this.lines,
  });

  factory Invoice.fromJson(Map<String, dynamic> json) => Invoice(
    invoiceNumber: (json['invoice_number'] as String?) ?? '',
    customerId: (json['customer_id'] as String?) ?? '',
    invoiceDate: (json['invoice_date'] as String?) ?? '',
    dueDate: (json['due_date'] as String?) ?? '',
    total: ExactAmount.fromJson(json['total_amount']),
    discount: ExactAmount.fromJson(json['discount_amount']),
    amountDue: json['amount_due'] == null
        ? null
        : ExactAmount.fromJson(json['amount_due']),
    paid: ExactAmount.fromJson(json['paid_amount']),
    status: (json['status'] as String?) ?? 'draft',
    lines: [
      for (final line in (json['items'] as List?) ?? const [])
        InvoiceLine(
          description:
              ((line as Map<String, dynamic>)['description'] as String?) ?? '',
          quantity: ExactAmount.fromJson(line['quantity']),
          unitPrice: ExactAmount.fromJson(line['unit_price']),
        ),
    ],
  );

  /// What the customer owes once the discount is off, which is the figure the sale carries to the ledger and the figure
  /// tax follows.
  ExactAmount get due => amountDue ?? (total - discount);

  /// What has still to come in on this invoice: what is owed once the discount is off, less what has been paid.
  ExactAmount get outstanding => due - paid;

  /// Whether this invoice is money the customer has actually been asked for.
  ///
  /// A draft has not been sent and a void one was withdrawn, so neither is a debt: counting them would chase money nobody
  /// was ever billed. Every total in the app - who owes what, how late it is, a customer's account, the register - adds up
  /// this figure rather than [outstanding], so a draft can never inflate what a business believes it is owed.
  bool get isClaim => status != 'draft' && status != 'void';

  /// What this invoice adds to what the customer owes: its outstanding amount if it has been asked for, otherwise nothing.
  ExactAmount get claim => isClaim ? outstanding : ExactAmount.zero;
}

/// What the engine says after a payment is recorded against an invoice.
///
/// [journalPosted] is the engine being honest: recording a payment against an invoice does not write a ledger entry, so a
/// screen that said "paid" without saying that would leave a Book whose income statement never saw the money.
class InvoicePayment {
  final String invoiceNumber;
  final ExactAmount amount;
  final ExactAmount paid;
  final ExactAmount outstanding;
  final String status;
  final bool journalPosted;

  const InvoicePayment({
    required this.invoiceNumber,
    required this.amount,
    required this.paid,
    required this.outstanding,
    required this.status,
    required this.journalPosted,
  });

  factory InvoicePayment.fromJson(Map<String, dynamic> json) => InvoicePayment(
    invoiceNumber: (json['invoice_number'] as String?) ?? '',
    amount: ExactAmount.fromJson(json['payment_amount']),
    paid: ExactAmount.fromJson(json['paid_amount']),
    outstanding: ExactAmount.fromJson(json['outstanding']),
    status: (json['status'] as String?) ?? '',
    journalPosted: json['journal_posted'] == true,
  );

  /// True once the invoice is settled.
  bool get settled => outstanding.isZero;
}

/// What one customer owes a Book, across every invoice raised against them.
class CustomerOwing {
  final String customerId;
  final String name;
  final ExactAmount owed;
  final int invoices;
  final String earliestDue;

  const CustomerOwing({
    required this.customerId,
    required this.name,
    required this.owed,
    required this.invoices,
    required this.earliestDue,
  });
}

/// One payment taken against an invoice, as the statement needs it: the day, and the amount.
class InvoicePaymentEntry {
  final String date;
  final ExactAmount amount;

  const InvoicePaymentEntry({required this.date, required this.amount});
}

/// One line of a customer statement: something that happened on a day, and where it left the balance.
class StatementLine {
  final String date;
  final String description;
  final ExactAmount charge;
  final ExactAmount credit;
  final ExactAmount running;

  const StatementLine({
    required this.date,
    required this.description,
    required this.charge,
    required this.credit,
    required this.running,
  });
}

/// A customer's statement: every invoice raised and every payment taken, in the order they happened.
///
/// A statement is a history, not a summary, so an invoice that has since been settled is still on it - that is the whole
/// point of sending somebody their account. The closing figure is the register's own outstanding total for that customer,
/// reached by adding up the same events, so the two cannot disagree.
class CustomerStatement {
  final String customerId;
  final String name;
  final List<StatementLine> lines;
  final ExactAmount charged;
  final ExactAmount paid;
  final ExactAmount closing;
  final ExactAmount late;
  final int oldestDaysLate;
  final String asAt;

  const CustomerStatement({
    required this.customerId,
    required this.name,
    required this.lines,
    required this.charged,
    required this.paid,
    required this.closing,
    required this.late,
    required this.oldestDaysLate,
    required this.asAt,
  });

  bool get isEmpty => lines.isEmpty;
}

/// A customer's statement, worked out from the register and the payments against it.
///
/// Dates are compared as dates, and an event with no readable date sorts first rather than vanishing: a payment that lost its
/// date is still money, and a statement that quietly dropped it would close higher than the Book does.
CustomerStatement customerStatement(
  List<Invoice> held,
  Map<String, List<InvoicePaymentEntry>> payments, {
  required String customerId,
  required String today,
  String? name,
}) {
  final events =
      <({String date, bool charge, String description, ExactAmount amount})>[];
  final mine = [
    for (final invoice in held)
      if (invoice.customerId == customerId) invoice,
  ];
  final invoicesByNumber = <String, Invoice>{};
  for (final invoice in mine) {
    invoicesByNumber[invoice.invoiceNumber] = invoice;
    events.add((
      date: invoice.invoiceDate,
      charge: true,
      description: '${invoice.invoiceNumber} raised, due ${invoice.dueDate}',
      amount: invoice.total,
    ));
  }
  for (final entry in payments.entries) {
    final invoice = invoicesByNumber[entry.key];
    if (invoice == null) continue;
    for (final payment in entry.value) {
      events.add((
        date: payment.date,
        charge: false,
        description: 'Payment against ${invoice.invoiceNumber}',
        amount: payment.amount,
      ));
    }
  }

  int byDate(
    ({String date, bool charge, String description, ExactAmount amount}) a,
    ({String date, bool charge, String description, ExactAmount amount}) b,
  ) {
    final dates = a.date.compareTo(b.date);
    if (dates != 0) return dates;
    // an invoice comes before the payment that settles it on the same day
    if (a.charge != b.charge) return a.charge ? -1 : 1;
    return a.description.compareTo(b.description);
  }

  events.sort(byDate);

  var running = ExactAmount.zero;
  var charged = ExactAmount.zero;
  var paid = ExactAmount.zero;
  final lines = <StatementLine>[];
  for (final event in events) {
    if (event.charge) {
      running = running + event.amount;
      charged = charged + event.amount;
    } else {
      running = running - event.amount;
      paid = paid + event.amount;
    }
    lines.add(
      StatementLine(
        date: event.date,
        description: event.description,
        charge: event.charge ? event.amount : ExactAmount.zero,
        credit: event.charge ? ExactAmount.zero : event.amount,
        running: running,
      ),
    );
  }

  var late = ExactAmount.zero;
  var oldest = 0;
  for (final invoice in mine) {
    final owing = invoice.claim;
    if (owing.isZero) continue;
    final days = daysBetween(invoice.dueDate, today);
    if (days <= 0) continue;
    late = late + owing;
    if (days > oldest) oldest = days;
  }

  return CustomerStatement(
    customerId: customerId,
    name: name ?? (mine.isEmpty ? customerId : customerId),
    lines: lines,
    charged: charged,
    paid: paid,
    closing: running,
    late: late,
    oldestDaysLate: oldest,
    asAt: today,
  );
}

/// One band of lateness, and what falls in it.
///
/// The bands are the ordinary ones a chase is run in: money not yet due is not a problem, and after that every month that
/// passes makes a debt both less likely to be paid and more worth a phone call.
class AgeingBand {
  final String label;
  final ExactAmount amount;
  final int invoices;

  const AgeingBand({
    required this.label,
    required this.amount,
    required this.invoices,
  });

  bool get isEmpty => amount.isZero;
}

/// What one customer owes, split by how late each invoice is.
class AgeingRow {
  final String customerId;
  final String name;
  final List<AgeingBand> bands;
  final ExactAmount owed;
  final ExactAmount late;
  final int oldestDaysLate;

  const AgeingRow({
    required this.customerId,
    required this.name,
    required this.bands,
    required this.owed,
    required this.late,
    required this.oldestDaysLate,
  });
}

/// The whole Book's lateness: who owes what, and how much of it has run past its due date.
class AgeingReport {
  final List<AgeingRow> rows;
  final List<AgeingBand> bands;

  const AgeingReport({required this.rows, required this.bands});

  ExactAmount get owed =>
      bands.fold(ExactAmount.zero, (running, band) => running + band.amount);

  /// Everything past its due date, which is the money a chase is actually about.
  ExactAmount get late => bands
      .skip(1)
      .fold(ExactAmount.zero, (running, band) => running + band.amount);

  bool get isEmpty => owed.isZero;

  AgeingRow? get worst {
    AgeingRow? worst;
    for (final row in rows) {
      if (row.late.isZero) continue;
      if (worst == null || row.oldestDaysLate > worst.oldestDaysLate) {
        worst = row;
      }
    }
    return worst;
  }
}

/// The banding a chase is run in, named once so the screen and the tests cannot drift apart.
const ageingBandLabels = [
  'Not yet due',
  '1–30 days late',
  '31–60 days late',
  '61–90 days late',
  'Over 90 days late',
];

/// How late the money is, worked out from the invoice register.
///
/// Every figure comes from invoices the engine already answered - the outstanding amount and the date each one was due - so
/// this cannot drift from the register. A settled invoice is not in it at all, and an invoice with no due date is counted as
/// not yet due rather than as late, because a debt is only late against a date somebody agreed.
AgeingReport ageingReport(
  List<Invoice> held, {
  required String today,
  Map<String, String> names = const {},
}) {
  final owed = <String, List<ExactAmount>>{};
  final counted = <String, List<int>>{};
  final oldest = <String, int>{};
  final byCustomer = <String, String>{};

  int band(int daysLate) {
    if (daysLate <= 0) return 0;
    if (daysLate <= 30) return 1;
    if (daysLate <= 60) return 2;
    if (daysLate <= 90) return 3;
    return 4;
  }

  for (final invoice in held) {
    final owing = invoice.claim;
    if (owing.isZero) continue;
    final who = invoice.customerId;
    byCustomer[who] = who;
    final amounts = owed.putIfAbsent(
      who,
      () => List<ExactAmount>.filled(5, ExactAmount.zero),
    );
    final counts = counted.putIfAbsent(who, () => List<int>.filled(5, 0));
    final daysLate = daysBetween(invoice.dueDate, today);
    final at = band(daysLate);
    amounts[at] = amounts[at] + owing;
    counts[at] = counts[at] + 1;
    if (daysLate > (oldest[who] ?? 0)) oldest[who] = daysLate;
  }

  final totals = List<ExactAmount>.filled(5, ExactAmount.zero);
  final totalCounts = List<int>.filled(5, 0);
  final rows = <AgeingRow>[];
  for (final entry in owed.entries) {
    final who = entry.key;
    final bands = [
      for (var i = 0; i < 5; i++)
        AgeingBand(
          label: ageingBandLabels[i],
          amount: entry.value[i],
          invoices: counted[who]![i],
        ),
    ];
    for (var i = 0; i < 5; i++) {
      totals[i] = totals[i] + entry.value[i];
      totalCounts[i] = totalCounts[i] + counted[who]![i];
    }
    final total = entry.value.fold(ExactAmount.zero, (a, b) => a + b);
    final late = entry.value.skip(1).fold(ExactAmount.zero, (a, b) => a + b);
    rows.add(
      AgeingRow(
        customerId: who,
        name: names[who] ?? who,
        bands: bands,
        owed: total,
        late: late,
        oldestDaysLate: oldest[who] ?? 0,
      ),
    );
  }
  // The people with money past its due date come first, worst first: that is the order a chase happens in.
  rows.sort((a, b) {
    if (a.late.isZero != b.late.isZero) return a.late.isZero ? 1 : -1;
    if (a.late.isZero) return b.owed.compareTo(a.owed);
    if (a.oldestDaysLate != b.oldestDaysLate) {
      return b.oldestDaysLate.compareTo(a.oldestDaysLate);
    }
    return b.late.compareTo(a.late);
  });
  return AgeingReport(
    rows: rows,
    bands: [
      for (var i = 0; i < 5; i++)
        AgeingBand(
          label: ageingBandLabels[i],
          amount: totals[i],
          invoices: totalCounts[i],
        ),
    ],
  );
}

/// Whole days from one date to another, both written as yyyy-MM-dd.
///
/// Dates are compared as dates rather than as strings, because '2026-9-1' and '2026-09-01' sort differently as text and the
/// same as days. An unreadable date counts as no time at all rather than as a large lateness.
int daysBetween(String from, String to) {
  final start = DateTime.tryParse(from.trim());
  final end = DateTime.tryParse(to.trim());
  if (start == null || end == null) return 0;
  return DateTime(
    end.year,
    end.month,
    end.day,
  ).difference(DateTime(start.year, start.month, start.day)).inDays;
}

/// Who owes what, worked out from the register the engine already answered.
///
/// It is a summary of the invoices rather than a second figure kept anywhere: an outstanding balance that the app
/// remembered would be a number to reconcile against the register, and the register is the one that is right.
List<CustomerOwing> owingByCustomer(
  List<Invoice> held, {
  Map<String, String> names = const {},
}) {
  final owed = <String, ExactAmount>{};
  final counted = <String, int>{};
  final earliest = <String, String>{};
  for (final invoice in held) {
    final owing = invoice.claim;
    if (owing.isZero) continue;
    final who = invoice.customerId;
    owed[who] = (owed[who] ?? ExactAmount.zero) + owing;
    counted[who] = (counted[who] ?? 0) + 1;
    final due = invoice.dueDate;
    if (due.isNotEmpty &&
        (!earliest.containsKey(who) || due.compareTo(earliest[who]!) < 0)) {
      earliest[who] = due;
    }
  }
  final rows = [
    for (final entry in owed.entries)
      CustomerOwing(
        customerId: entry.key,
        name: names[entry.key] ?? entry.key,
        owed: entry.value,
        invoices: counted[entry.key] ?? 0,
        earliestDue: earliest[entry.key] ?? '',
      ),
  ];
  rows.sort((a, b) => b.owed.compareTo(a.owed));
  return rows;
}

/// The invoicing doors of one Book.
/// What the whole register still has to bring in, added from the same figure each invoice shows.
///
/// One place, because two places would answer "how much is owed" differently the first time one of them was edited: the
/// register's heading and the "where you stand" page both ask this.
ExactAmount owedAcross(Iterable<Invoice> invoices) =>
    ExactAmount.sum(invoices.map((invoice) => invoice.claim));

class InvoicingService {
  InvoicingService._();

  static final InvoicingService instance = InvoicingService._();

  /// A second instance for a test that needs a transport of its own.
  InvoicingService.fresh();

  static const String _base = AppConfig.apiUrl;

  http.Client _client = http.Client();

  /// A test calls the service against a transport it controls. Nothing in the app sets this.
  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  /// A test may act as a particular session; the app leaves this alone so the session's own token is used.
  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// The Book's customers.
  Future<List<InvoiceCustomer>> listCustomers() async {
    final answer = await _client.get(
      Uri.parse('$_base/customers'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final rows = jsonDecode(answer.body) as List;
    return [
      for (final row in rows)
        InvoiceCustomer.fromJson(row as Map<String, dynamic>),
    ];
  }

  /// Add a customer, so an invoice can name them.
  ///
  /// The identifier is the Book's own and is what an invoice points at, so it is derived from the name once and never
  /// changed afterwards: two customers sharing one identifier would split a debtor's history between them.
  Future<InvoiceCustomer> createCustomer(String name) async {
    final trimmed = name.trim();
    if (trimmed.isEmpty) {
      throw Exception('A customer needs a name.');
    }
    final answer = await _client.post(
      Uri.parse('$_base/customers'),
      headers: _headers,
      body: jsonEncode({
        'customer_id': _identifierFor(trimmed),
        'name': trimmed,
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine refused the customer: ${answer.statusCode} ${answer.body}',
      );
    }
    return InvoiceCustomer.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Change a customer's details.
  ///
  /// The name may be corrected; the identifier never is, because that is what every invoice already points at. A misspelt
  /// name is exactly what this is for, so it is the one thing the register lets a person fix.
  Future<InvoiceCustomer> updateCustomer(
    String customerId, {
    required String name,
    String? email,
    String? phone,
    String? address,
  }) async {
    final trimmed = name.trim();
    if (trimmed.isEmpty) {
      throw Exception('A customer needs a name.');
    }
    final answer = await _client.put(
      Uri.parse('$_base/customers/$customerId'),
      headers: _headers,
      body: jsonEncode({
        'name': trimmed,
        'email': email ?? '',
        'phone': phone ?? '',
        'address': address ?? '',
      }),
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine refused the change: ${answer.statusCode} ${answer.body}',
      );
    }
    return InvoiceCustomer.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// Forget a customer.
  ///
  /// Only ever offered for a customer with no invoices: the engine would delete one that has them, and every invoice they
  /// were billed on would be left pointing at nothing.
  Future<void> removeCustomer(String customerId) async {
    final answer = await _client.delete(
      Uri.parse('$_base/customers/$customerId'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine refused to remove the customer: ${answer.statusCode} ${answer.body}',
      );
    }
  }

  /// Every invoice the Book has raised, newest first as the engine returns them.
  Future<List<Invoice>> listInvoices() async {
    final answer = await _client.get(
      Uri.parse('$_base/invoices'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final rows = jsonDecode(answer.body) as List;
    return [
      for (final row in rows) Invoice.fromJson(row as Map<String, dynamic>),
    ];
  }

  /// Raise an invoice.
  ///
  /// The total is summed here from the lines rather than typed in, because the engine refuses a total that disagrees with
  /// its lines: a figure the phone worked out cannot be the reason a correct invoice is turned away.
  Future<Invoice> raiseInvoice({
    required String invoiceNumber,
    required InvoiceCustomer customer,
    required List<InvoiceLine> lines,
    required DateTime invoiceDate,
    required DateTime dueDate,
    String notes = '',
    String discountAmount = '',
    List<String> discountRules = const [],
    String currency = '',
    String exchangeRate = '',
  }) async {
    final number = invoiceNumber.trim();
    if (number.isEmpty) {
      throw Exception('An invoice needs a number.');
    }
    if (lines.isEmpty) {
      throw Exception('An invoice needs at least one line.');
    }
    final total = ExactAmount.sum(lines.map((line) => line.amount));
    // an invoice for nothing is a mistyped line rather than a bill: the engine takes a zero total as faithfully as any other
    // figure, and a customer billed 0.00 is a debt that will never be chased
    if (total.isZero) {
      throw Exception(
        'Every line came to nothing, so this would bill 0.00. Check the '
        'quantity and the price.',
      );
    }
    final answer = await _client.post(
      Uri.parse('$_base/invoices'),
      headers: _headers,
      body: jsonEncode({
        'invoice_number': number,
        'customer_id': customer.customerId,
        'invoice_date': _date(invoiceDate),
        'due_date': _date(dueDate),
        'items': [for (final line in lines) line.toJson()],
        'total_amount': total.toLedgerString(),
        'status': 'outstanding',
        'notes': notes,
        // a discount is named rather than worked out here: the engine takes the compulsory rules that fit, then the
        // offered ones named, and answers with what the customer actually owes
        if (discountRules.isNotEmpty) 'discount_rules': discountRules,
        if (discountAmount.trim().isNotEmpty)
          'discount_amount': discountAmount.trim(),
        // the money the customer is billed in, and the rate it was raised at. Both are read by the engine's one rule, so
        // a rate it would refuse is refused here rather than posting a figure whose money nobody can tell
        if (currency.trim().isNotEmpty)
          'currency': currency.trim().toUpperCase(),
        if (exchangeRate.trim().isNotEmpty)
          'exchange_rate': exchangeRate.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine refused the invoice: ${answer.statusCode} ${answer.body}',
      );
    }
    return Invoice.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Record what a customer has paid against an invoice.
  ///
  /// The amount is compared with what is still owing before it is sent, because paying an invoice twice is the mistake
  /// that costs somebody money and the engine, faithful to its own rule, would take it: it adds whatever it is given.
  Future<InvoicePayment> recordPayment(
    Invoice invoice,
    ExactAmount amount, {
    DateTime? on,
  }) async {
    if (amount.isZero) {
      throw Exception('A payment has to be more than nothing.');
    }
    if (amount > invoice.outstanding) {
      throw Exception(
        'That is more than the ${invoice.outstanding.toDecimalString()} still owing on ${invoice.invoiceNumber}.',
      );
    }
    final answer = await _client.post(
      Uri.parse(
        '$_base/invoices/${Uri.encodeComponent(invoice.invoiceNumber)}/record-payment',
      ),
      headers: _headers,
      body: jsonEncode({
        'payment_amount': amount.toLedgerString(),
        if (on != null) 'payment_date': _date(on),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine refused the payment: ${answer.statusCode} ${answer.body}',
      );
    }
    return InvoicePayment.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  /// What has been paid against an invoice so far, with the day each payment was taken.
  ///
  /// A statement is a list of events in date order, so the day matters as much as the amount: two payments off one invoice
  /// dated a month apart are two lines on it, not one figure.
  Future<List<InvoicePaymentEntry>> paymentEntriesFor(
    String invoiceNumber,
  ) async {
    final answer = await _client.get(
      Uri.parse(
        '$_base/invoices/${Uri.encodeComponent(invoiceNumber)}/payments',
      ),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final rows = jsonDecode(answer.body) as List;
    return [
      for (final row in rows)
        InvoicePaymentEntry(
          date:
              ((row as Map<String, dynamic>)['payment_date'] as String?) ?? '',
          amount: ExactAmount.fromJson(row['amount']),
        ),
    ];
  }

  /// What has been paid against an invoice so far.
  Future<List<ExactAmount>> paymentsFor(String invoiceNumber) async => [
    for (final entry in await paymentEntriesFor(invoiceNumber)) entry.amount,
  ];

  /// The next number to offer, so two invoices do not collide by accident.
  ///
  /// It reads the register rather than keeping a counter on the phone: the Book already knows which numbers are taken,
  /// including the ones raised somewhere else, and a counter here would be wrong on the second device.
  String nextNumberFor(List<Invoice> held) {
    var highest = 0;
    for (final invoice in held) {
      final digits = RegExp(r'(\d+)\s*$').firstMatch(invoice.invoiceNumber);
      final number = int.tryParse(digits?.group(1) ?? '');
      if (number != null && number > highest) highest = number;
    }
    return 'INV-${(highest + 1).toString().padLeft(4, '0')}';
  }

  static String _date(DateTime when) =>
      '${when.year.toString().padLeft(4, '0')}-'
      '${when.month.toString().padLeft(2, '0')}-'
      '${when.day.toString().padLeft(2, '0')}';

  static String _identifierFor(String name) {
    final slug = name
        .toLowerCase()
        .replaceAll(RegExp(r'[^a-z0-9]+'), '-')
        .replaceAll(RegExp(r'^-+|-+$'), '');
    return slug.isEmpty ? 'customer' : slug;
  }
}
