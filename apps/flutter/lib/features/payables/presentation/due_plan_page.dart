// When you must pay: the money owed to suppliers, in the order it has to leave the account.
//
// The page exists because a single total owed is the wrong question at the moment somebody sits down to pay suppliers.
// What they need is the order: what is already late, what falls due this week, and what can wait until the month turns.
// The engine places the bills by their own due dates, and this page shows that placing.
//
// Nothing is hidden to make the page look tidier. A supplier whose bill carries no due date is listed with that said
// plainly rather than left out, and a read that failed is reported as a failure rather than as a Book owing nothing.
import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/payables/data/due_plan.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/supplier_account_page.dart';

class DuePlanPage extends StatefulWidget {
  const DuePlanPage({super.key, this.service, this.today});

  final DuePlanService? service;

  /// The day to answer as at. Fixed in a test so the page cannot drift with the clock.
  final String? today;

  @override
  State<DuePlanPage> createState() => _DuePlanPageState();
}

class _DuePlanPageState extends State<DuePlanPage> {
  late final DuePlanService _service =
      widget.service ?? DuePlanService.instance;

  DuePlan? _plan;
  String? _failure;
  bool _reading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  String get _today {
    final fixed = widget.today;
    if (fixed != null && fixed.isNotEmpty) return fixed;
    final now = DateTime.now();
    final month = now.month.toString().padLeft(2, '0');
    final day = now.day.toString().padLeft(2, '0');
    return '${now.year}-$month-$day';
  }

  Future<void> _load() async {
    setState(() {
      _reading = true;
      _failure = null;
    });
    try {
      final plan = await _service.plan(_today);
      if (!mounted) return;
      setState(() {
        _plan = plan;
        _reading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _failure = error.toString();
        _reading = false;
      });
    }
  }

  Future<void> _openAccount(SupplierDue supplier) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => SupplierAccountPage(
          supplierId: supplier.supplierId,
          supplierName: supplier.name,
        ),
      ),
    );
    // paying one may have cleared it, so the order is read again rather than assumed
    if (mounted) await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('When you must pay'),
        actions: [
          IconButton(
            onPressed: _reading ? null : _load,
            tooltip: 'Read again',
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: RefreshIndicator(onRefresh: _load, child: _body(context)),
    );
  }

  Widget _body(BuildContext context) {
    if (_reading && _plan == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final failure = _failure;
    if (failure != null && _plan == null) {
      return CouldNotRead(
        what: 'What you owe suppliers',
        error: failure,
        onRetry: _load,
      );
    }
    final plan = _plan;
    if (plan == null) return const SizedBox.shrink();

    if (plan.nothingOwed) {
      return ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(plan.headline, style: Theme.of(context).textTheme.headlineSmall),
          const SizedBox(height: 8),
          const Text(
            'Bills you record against a supplier show up here with the day they '
            'fall due, so you can see what to pay first.',
          ),
        ],
      );
    }

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(plan.headline, style: Theme.of(context).textTheme.headlineSmall),
        if (plan.detail.isNotEmpty) ...[
          const SizedBox(height: 6),
          Text(plan.detail, style: Theme.of(context).textTheme.bodyMedium),
        ],
        const SizedBox(height: 16),
        _bucket(context, 'Already late', plan.overdue, late: true),
        _bucket(context, 'Due within a week', plan.dueSoon),
        _bucket(context, 'Can wait', plan.later),
        if (plan.undated > ExactAmount.zero)
          _bucket(context, 'No due date on the bill', plan.undated),
        const SizedBox(height: 20),
        Text(
          'In the order to pay',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: 8),
        for (final supplier in plan.suppliers)
          Card(
            child: ListTile(
              title: Text(supplier.name),
              subtitle: Text(supplier.timing),
              trailing: Text(
                supplier.owed.toDecimalString(),
                style: Theme.of(context).textTheme.titleMedium,
              ),
              onTap: () => _openAccount(supplier),
            ),
          ),
      ],
    );
  }

  Widget _bucket(
    BuildContext context,
    String label,
    ExactAmount amount, {
    bool late = false,
  }) {
    return Card(
      color: late
          ? Theme.of(context).colorScheme.errorContainer.withValues(alpha: 0.4)
          : null,
      child: ListTile(
        title: Text(label),
        trailing: Text(
          amount.toDecimalString(),
          style: Theme.of(context).textTheme.titleMedium,
        ),
      ),
    );
  }
}
