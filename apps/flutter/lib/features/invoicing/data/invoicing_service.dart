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

  const InvoiceCustomer({
    required this.id,
    required this.customerId,
    required this.name,
  });

  factory InvoiceCustomer.fromJson(Map<String, dynamic> json) =>
      InvoiceCustomer(
        id: (json['id'] as String?) ?? '',
        customerId: (json['customer_id'] as String?) ?? '',
        name: (json['name'] as String?) ?? '',
      );

  /// What a list shows when a customer was saved without a name.
  String get label => name.isNotEmpty ? name : customerId;
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
  final ExactAmount paid;
  final String status;
  final List<InvoiceLine> lines;

  const Invoice({
    required this.invoiceNumber,
    required this.customerId,
    required this.invoiceDate,
    required this.dueDate,
    required this.total,
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

  /// What has still to come in on this invoice.
  ExactAmount get outstanding => total - paid;
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

/// The invoicing doors of one Book.
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
  }) async {
    final number = invoiceNumber.trim();
    if (number.isEmpty) {
      throw Exception('An invoice needs a number.');
    }
    if (lines.isEmpty) {
      throw Exception('An invoice needs at least one line.');
    }
    final total = ExactAmount.sum(lines.map((line) => line.amount));
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

  /// What has been paid against an invoice so far.
  Future<List<ExactAmount>> paymentsFor(String invoiceNumber) async {
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
        ExactAmount.fromJson((row as Map<String, dynamic>)['amount']),
    ];
  }

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
