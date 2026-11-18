// Everyone you bill: the customer register, read as a list of people rather than as invoice numbers.
//
// The engine has kept customers all along, but the only place one appeared was inside an invoice, so a customer who had
// nothing owing could not be found, a misspelt name could not be corrected, and there was no way to see who was on the books at
// all. Nothing here is stored: what each customer owes is summed from the same invoices the register shows, because a balance
// the app kept for itself would be a second figure to reconcile against them.
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/invoicing/data/invoicing_service.dart';

/// One customer as the register holds them, with what the invoices say about them.
class CustomerOnTheBooks {
  const CustomerOnTheBooks({
    required this.customer,
    required this.owed,
    required this.invoices,
    required this.lastBilledOn,
  });

  final InvoiceCustomer customer;

  /// What they still owe across every invoice. Zero for a customer who has paid, or has never been billed.
  final ExactAmount owed;

  /// How many invoices have ever been raised against them, paid or not.
  final int invoices;

  /// The most recent invoice date, or empty when they have never been billed.
  final String lastBilledOn;

  String get name => customer.label;

  /// A customer who has never been billed is not the same as one who has paid: the first has no history to lose.
  bool get neverBilled => invoices == 0;

  /// Why this customer may not be removed, or null when they may be.
  ///
  /// The engine would delete a customer who has invoices and leave every one of them pointing at an identifier nothing
  /// answers for, so the refusal is here, before anything is sent.
  String? get removalRefusal {
    if (neverBilled) return null;
    final plural = invoices == 1 ? 'invoice' : 'invoices';
    return '$name has $invoices $plural on file. Removing them would leave '
        '${invoices == 1 ? 'that invoice' : 'those invoices'} pointing at nobody.';
  }
}

/// Every customer on the books, read against the invoices.
///
/// Customers who owe money come first, largest debt first, because that is the order somebody chasing money wants. Everyone
/// else follows alphabetically, so a customer with nothing owing is still findable.
List<CustomerOnTheBooks> registerOf(
  List<InvoiceCustomer> customers,
  List<Invoice> invoices,
) {
  final owed = <String, ExactAmount>{};
  final counted = <String, int>{};
  final lastBilled = <String, String>{};

  for (final invoice in invoices) {
    final who = invoice.customerId;
    counted[who] = (counted[who] ?? 0) + 1;
    final owing = invoice.claim;
    if (!owing.isZero) {
      owed[who] = (owed[who] ?? ExactAmount.zero) + owing;
    }
    final raised = invoice.invoiceDate;
    if (raised.isNotEmpty &&
        (!lastBilled.containsKey(who) ||
            raised.compareTo(lastBilled[who]!) > 0)) {
      lastBilled[who] = raised;
    }
  }

  final rows = [
    for (final customer in customers)
      CustomerOnTheBooks(
        customer: customer,
        owed: owed[customer.customerId] ?? ExactAmount.zero,
        invoices: counted[customer.customerId] ?? 0,
        lastBilledOn: lastBilled[customer.customerId] ?? '',
      ),
  ];

  rows.sort((a, b) {
    final byDebt = b.owed.compareTo(a.owed);
    if (byDebt != 0) return byDebt;
    return a.name.toLowerCase().compareTo(b.name.toLowerCase());
  });
  return rows;
}

/// The customer already on the books under this name, or null when the name is free.
///
/// Two spellings of one customer is the thing a register exists to prevent, so a name already in use is answered with the
/// customer who holds it rather than quietly creating a second one. Case and spacing are ignored: "Acme Ltd" and "acme ltd "
/// are the same customer to a person, and would split their history in two.
CustomerOnTheBooks? nameAlreadyTaken(
  String wanted,
  List<CustomerOnTheBooks> held,
) {
  final looking = _same(wanted);
  if (looking.isEmpty) return null;
  for (final row in held) {
    if (_same(row.name) == looking) return row;
  }
  return null;
}

String _same(String name) =>
    name.trim().toLowerCase().replaceAll(RegExp(r'\s+'), ' ');
