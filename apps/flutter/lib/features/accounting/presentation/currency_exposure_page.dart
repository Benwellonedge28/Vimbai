// What the business still holds in each money: the screen an owner opens before deciding what to pay or chase.
//
// Two directions, kept apart. Money owed to the business in dollars and money the business owes in dollars are both in
// dollars, but one is coming and one is going, and adding them would describe neither. Each money stands on its own line
// for the same reason: a hundred dollars and a hundred pounds are not two hundred of anything.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/currency_exposure.dart';

class CurrencyExposurePage extends StatefulWidget {
  const CurrencyExposurePage({super.key, this.service});

  /// A test passes its own service so the page can be exercised without a server.
  final CurrencyExposureService? service;

  @override
  State<CurrencyExposurePage> createState() => _CurrencyExposurePageState();
}

class _CurrencyExposurePageState extends State<CurrencyExposurePage> {
  late final CurrencyExposureService _service =
      widget.service ?? CurrencyExposureService.instance;

  bool _loading = true;
  String? _error;
  CurrencyExposureReport? _report;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final report = await _service.read();
      if (!mounted) return;
      setState(() {
        _report = report;
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

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('What you hold in each money'),
        actions: [
          IconButton(
            tooltip: 'Read it again',
            onPressed: _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: _body(),
    );
  }

  Widget _body() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'What you hold in each money',
        error: _error!,
        onRetry: _load,
      );
    }
    final report = _report;
    if (report == null) {
      return CouldNotRead(
        what: 'What you hold in each money',
        error: 'The reading came back empty.',
        onRetry: _load,
      );
    }
    final theme = Theme.of(context);
    if (report.isQuiet) {
      return Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Everything is in your own money',
              style: theme.textTheme.titleLarge,
            ),
            const SizedBox(height: 8),
            Text(
              'No invoice or bill has been raised in another money, so there '
              'is nothing here to keep an eye on.',
              style: theme.textTheme.bodyMedium,
            ),
          ],
        ),
      );
    }

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        for (final currency in report.currencies) ...[
          _CurrencyCard(exposure: currency),
          const SizedBox(height: 12),
        ],
        if (report.unmatched.isNotEmpty) ...[
          const SizedBox(height: 8),
          Text('Payments naming nothing', style: theme.textTheme.titleMedium),
          const SizedBox(height: 4),
          Text(
            'These were recorded in another money but name no document this '
            'Book holds, so they have not been set against anything. They are '
            'listed rather than spread across other bills.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 8),
          for (final one in report.unmatched)
            ListTile(
              dense: true,
              contentPadding: EdgeInsets.zero,
              title: Text(one.document.isEmpty ? 'No reference' : one.document),
              subtitle: Text('${one.units.toDecimalString()} ${one.currency}'),
            ),
        ],
      ],
    );
  }
}

class _CurrencyCard extends StatelessWidget {
  const _CurrencyCard({required this.exposure});

  final CurrencyExposure exposure;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(exposure.currency, style: theme.textTheme.titleLarge),
            const SizedBox(height: 8),
            if (exposure.isClear)
              Text(
                'Nothing is still owed either way in ${exposure.currency}.',
                style: theme.textTheme.bodyMedium,
              )
            else ...[
              if (exposure.owedToTheBusiness.isNotEmpty)
                _Direction(
                  title: 'Owed to you',
                  units: exposure.owedToTheBusinessUnits,
                  carried: exposure.owedToTheBusinessCarried,
                  currency: exposure.currency,
                  documents: exposure.owedToTheBusiness,
                ),
              if (exposure.owedByTheBusiness.isNotEmpty) ...[
                if (exposure.owedToTheBusiness.isNotEmpty)
                  const SizedBox(height: 12),
                _Direction(
                  title: 'You owe',
                  units: exposure.owedByTheBusinessUnits,
                  carried: exposure.owedByTheBusinessCarried,
                  currency: exposure.currency,
                  documents: exposure.owedByTheBusiness,
                ),
              ],
            ],
          ],
        ),
      ),
    );
  }
}

class _Direction extends StatelessWidget {
  const _Direction({
    required this.title,
    required this.units,
    required this.carried,
    required this.currency,
    required this.documents,
  });

  final String title;
  final ExactAmount units;
  final ExactAmount carried;
  final String currency;
  final List<ForeignDocument> documents;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: theme.textTheme.titleMedium),
        const SizedBox(height: 2),
        Text(
          '${units.toDecimalString()} $currency',
          style: theme.textTheme.headlineSmall,
        ),
        Text(
          'Carried in this Book at ${carried.toDecimalString()}',
          style: theme.textTheme.bodySmall,
        ),
        const SizedBox(height: 6),
        for (final document in documents)
          ListTile(
            dense: true,
            contentPadding: EdgeInsets.zero,
            title: Text(
              document.reference.isEmpty ? document.date : document.reference,
            ),
            subtitle: Text(
              '${document.date} · at ${document.rate.toDecimalString()} · '
              '${document.outstandingUnits.toDecimalString()} $currency left',
            ),
          ),
      ],
    );
  }
}
