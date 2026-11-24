// Recording the money that actually left, when a supplier is paid.
//
// A payment recorded against a supplier is bookkeeping against the purchases ledger: the engine lowers what is owed and
// moves no money anywhere. So a business could owe 450.00, pay 450.00, and have the cash book still showing every penny
// sitting in the bank. The two records then describe different worlds, and the day somebody reconciles the account is the
// day they find out.
//
// This sheet is the step between them: the supplier's own figure, the account the money left, and what the payment clears.
// Nothing is assumed about which account it came out of, because guessing which bank account a payment left is the kind of
// guess that takes a month to find. The ledger entry is deliberately not written here: a cash movement has one route into
// the books, and that route is the payment itself, which carries its own reference so it can only be posted once.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/cashbook/data/cashbook_service.dart';
import 'package:vimbai_mobile_client/features/payables/data/bill_posting.dart';
import 'package:vimbai_mobile_client/features/payables/data/foreign_bill_payment.dart';

class PaySupplierOutSheet extends StatefulWidget {
  const PaySupplierOutSheet({
    super.key,
    required this.amount,
    required this.supplierName,
    required this.cash,
    required this.ledger,
    this.reference = '',
    this.onDate,
    this.vendorId = '',
    this.bills,
  });

  final ExactAmount amount;
  final String supplierName;
  final CashBookService cash;
  final AccountingApiService ledger;

  /// The bill's own reference, carried so the payment can be traced back to what it settles.
  final String reference;

  /// For a test to fix the day; today otherwise.
  final String? onDate;

  /// The supplier, so the bill behind this payment can be found and asked what money it was in.
  final String vendorId;

  /// The bills route, injectable so a test can answer without a server.
  final ForeignBillPaymentService? bills;

  static Future<CashEntry?> show(
    BuildContext context, {
    required ExactAmount amount,
    required String supplierName,
    required CashBookService cash,
    required AccountingApiService ledger,
    String reference = '',
    String vendorId = '',
    ForeignBillPaymentService? bills,
  }) => showModalBottomSheet<CashEntry>(
    context: context,
    isScrollControlled: true,
    builder: (_) => PaySupplierOutSheet(
      amount: amount,
      supplierName: supplierName,
      cash: cash,
      ledger: ledger,
      reference: reference,
      vendorId: vendorId,
      bills: bills,
    ),
  );

  @override
  State<PaySupplierOutSheet> createState() => _PaySupplierOutSheetState();
}

class _PaySupplierOutSheetState extends State<PaySupplierOutSheet> {
  bool _loading = true;
  bool _saving = false;
  String? _error;
  String? _complaint;
  List<CashAccount> _places = const [];
  Account? _against;
  CashAccount? _place;

  late final ForeignBillPaymentService _bills =
      widget.bills ?? ForeignBillPaymentService.instance;

  /// The bill behind this payment, when the supplier raised it in another money.
  BillInAnotherMoney? _abroad;

  /// What one of that money is worth the day it is paid, as the person gives it.
  final TextEditingController _todayRate = TextEditingController();

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _todayRate.dispose();
    super.dispose();
  }

  /// The rate on the day, or nothing when it does not read as a figure.
  ExactAmount? get _rateToday {
    final typed = _todayRate.text.trim();
    if (typed.isEmpty) return null;
    try {
      final rate = ExactAmount.parse(typed);
      return rate.isZero ? null : rate;
    } catch (_) {
      return null;
    }
  }

  /// What paying the bill would cost in the Book's money at the rate given, or nothing yet.
  ExactAmount? get _worthPaying {
    final abroad = _abroad;
    final rate = _rateToday;
    if (abroad == null || rate == null) return null;
    return abroad.worthAt(rate);
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final places = await widget.cash.accounts();
      final chart = await widget.ledger.getAllAccounts();
      // a bill raised in another money needs its own money and its own rate; a bill in this Book's money answers nothing,
      // and a read that fails leaves the ordinary payment exactly as it was
      BillInAnotherMoney? abroad;
      try {
        abroad = await _bills.find(
          vendorId: widget.vendorId,
          reference: widget.reference,
        );
      } catch (_) {
        abroad = null;
      }
      if (!mounted) return;
      setState(() {
        _places = [
          for (final account in places)
            if (account.type == 'cash' || account.type == 'bank') account,
        ];
        _against = payableAccount(chart);
        _place = _places.isEmpty ? null : _places.first;
        _abroad = abroad;
        _loading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _error = CouldNotRead.reasonOf(failure);
        _loading = false;
      });
    }
  }

  Future<void> _record() async {
    final place = _place;
    final against = _against;
    if (place == null) {
      setState(
        () => _complaint =
            'This Book has no cash or bank account, so there is nowhere for the money to have '
            'left from. Add one in the cash book first.',
      );
      return;
    }
    if (against == null) {
      setState(
        () => _complaint =
            'This Book has no Accounts Payable account, so this payment cannot be recorded '
            'against what you owe. Add one in Chart of accounts.',
      );
      return;
    }
    // a bill in another money is paid in its own money at the rate of the day, and the difference between what the books
    // carried and what actually left is posted on the payable side
    final abroad = _abroad;
    final rate = _rateToday;
    if (abroad != null && rate == null) {
      setState(
        () => _complaint =
            'Say what one ${abroad.currency} is worth today, so the payment can be '
            'recorded in the money it was billed in.',
      );
      return;
    }
    final worth = abroad == null ? null : abroad.worthAt(rate!);
    setState(() {
      _saving = true;
      _complaint = null;
    });
    final day =
        widget.onDate ?? DateTime.now().toIso8601String().substring(0, 10);
    try {
      final entry = await widget.cash.record(
        account: place,
        moneyIn: false,
        amount: abroad?.outstandingUnits ?? widget.amount,
        description: 'Paid ${widget.supplierName}'.trim(),
        date: day,
        voucher: widget.reference,
        againstAccount: against.accountNumber,
        currency: abroad?.currency ?? '',
        rate: abroad == null ? '' : rate!.toDecimalString(),
      );
      if (!mounted) return;
      if (abroad != null &&
          worth != null &&
          worth != abroad.outstandingCarried) {
        // the money has left, so the difference is posted after it: a posting that failed here must not stop the payment
        // having been recorded, and the message says which of the two happened
        await _bills.recordDifference(
          entryDate: day,
          reference: widget.reference,
          carried: abroad.outstandingCarried,
          received: worth,
        );
      }
      if (!mounted) return;
      Navigator.of(context).pop(entry);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _complaint = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Record ${widget.amount.toDecimalString()} leaving',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 4),
            Text(
              'The supplier is marked as paid. Record the money that left, so the cash book '
              'and what you owe describe the same event.',
            ),
            const SizedBox(height: 12),
            if (_loading) const LinearProgressIndicator(),
            if (!_loading && _abroad != null) ...[
              Text(
                '${_abroad!.billNumber} was billed in ${_abroad!.currency}: '
                '${_abroad!.outstandingUnits.toDecimalString()} '
                '${_abroad!.currency} still owed, carried at '
                '${_abroad!.outstandingCarried.toDecimalString()}.',
                style: Theme.of(context).textTheme.bodyMedium,
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _todayRate,
                keyboardType: const TextInputType.numberWithOptions(
                  decimal: true,
                ),
                decoration: InputDecoration(
                  labelText: 'What one ${_abroad!.currency} is worth today',
                  helperText: _worthPaying == null
                      ? 'The payment is recorded in the money it was billed in.'
                      : 'Paying the rest costs '
                            '${_worthPaying!.toDecimalString()} in this '
                            "Book's money.",
                ),
                onChanged: (_) => setState(() {}),
              ),
              const SizedBox(height: 8),
            ],
            if (_error != null)
              CouldNotRead(
                what: 'Where the money could have left from',
                error: _error,
                onRetry: _load,
              ),
            if (!_loading && _error == null) ...[
              DropdownButtonFormField<String>(
                initialValue: _place?.code,
                isExpanded: true,
                decoration: const InputDecoration(labelText: 'Money left'),
                items: [
                  for (final place in _places)
                    DropdownMenuItem(
                      value: place.code,
                      child: Text('${place.name} (${place.code})'),
                    ),
                ],
                onChanged: (code) {
                  for (final place in _places) {
                    if (place.code == code) setState(() => _place = place);
                  }
                },
              ),
              const SizedBox(height: 12),
              Text(
                _against == null
                    ? 'This Book has no Accounts Payable account to clear.'
                    : 'Against ${_against!.accountNumber} ${_against!.accountName}',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              const SizedBox(height: 12),
              Text(
                widget.reference.isEmpty
                    ? 'The day it is recorded is the day the money left.'
                    : 'Voucher ${widget.reference}, so the payment can be traced to the bill.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
            if (_complaint != null) ...[
              const SizedBox(height: 12),
              Text(
                _complaint!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _saving ? null : () => Navigator.of(context).pop(),
                  child: const Text('Not now'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _saving || _loading || _error != null
                      ? null
                      : _record,
                  child: Text(_saving ? 'Recording…' : 'Record it'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
