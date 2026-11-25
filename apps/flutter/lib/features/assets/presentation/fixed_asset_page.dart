// One thing the business owns: what it cost, what is left, and every month charged against it.
//
// This is where the writing off happens, so it is deliberately plain about what the next charge will be before anything is
// sent: the month, the amount, and what will be left afterwards. Charging a month twice would write the asset down twice,
// so a month already charged is refused here before the engine has to say so, and the engine's own refusal is shown in
// words if it happens anyway.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_asset_register.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_assets_service.dart';

class FixedAssetPage extends StatefulWidget {
  const FixedAssetPage({
    super.key,
    required this.assetId,
    this.service,
    this.month,
  });

  final String assetId;

  /// A test passes its own service so the page can be exercised without a server.
  final FixedAssetsService? service;

  /// A test fixes the month being charged, so it does not depend on the day it runs.
  final String? month;

  @override
  State<FixedAssetPage> createState() => _FixedAssetPageState();
}

class _FixedAssetPageState extends State<FixedAssetPage> {
  late final FixedAssetsService _assets =
      widget.service ?? FixedAssetsService.instance;

  late final String _month =
      widget.month ?? DateTime.now().toIso8601String().substring(0, 7);

  bool _loading = true;
  String? _error;
  FixedAsset? _asset;
  List<DepreciationCharge> _history = const [];
  bool _changed = false;

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
      final asset = await _assets.asset(widget.assetId);
      List<DepreciationCharge> history;
      try {
        history = await _assets.history(widget.assetId);
      } catch (_) {
        // the asset is the page; a history that could not be read is said so, not shown as no history
        history = const [];
      }
      if (!mounted) return;
      setState(() {
        _asset = asset;
        _history = history;
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

  Future<void> _charge() async {
    final asset = _asset;
    if (asset == null) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Charge $_month?'),
        content: Text(
          '${asset.chargeFor(_month).toDecimalString()} comes off what it is '
          'worth, leaving '
          '${(asset.netBookValue - asset.chargeFor(_month)).toDecimalString()}.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Not now'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Charge it'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await _assets.chargeMonth(id: asset.id, period: _month);
      _changed = true;
      await _load();
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('$_month charged and recorded.')));
    } catch (failure) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(CouldNotRead.reasonOf(failure))));
    }
  }

  Future<void> _dispose() async {
    final asset = _asset;
    if (asset == null) return;
    final done = await DisposeAssetSheet.show(
      context,
      service: _assets,
      asset: asset,
      today: _month,
    );
    if (done == true) {
      _changed = true;
      await _load();
    }
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        Navigator.of(context).pop(_changed);
      },
      child: Scaffold(
        appBar: AppBar(title: Text(_title())),
        body: _body(context),
      ),
    );
  }

  String _title() {
    final asset = _asset;
    if (asset == null) return 'One thing you own';
    return asset.name.isEmpty ? asset.code : asset.name;
  }

  Widget _body(BuildContext context) {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(what: 'this asset', error: _error, onRetry: _load);
    }
    final asset = _asset;
    if (asset == null) {
      return const Center(child: Text('This asset is not on the register.'));
    }
    final theme = Theme.of(context);
    final alreadyCharged = _history.any((charge) => charge.period == _month);
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
        children: [
          _standingCard(context, asset),
          const SizedBox(height: 16),
          if (!asset.isHeld)
            _note(
              context,
              'This one has been sold or scrapped, so nothing more is written '
              'off it.',
            )
          else if (asset.chargesNothing)
            _note(
              context,
              'No useful life was recorded, so the register writes nothing off '
              'this. It will sit at what it cost for ever.',
            )
          else if (asset.fullyWrittenDown)
            _note(
              context,
              'It is down to what it was expected to be worth at the end, so '
              'there is nothing left to write off.',
            )
          else
            _chargeCard(context, asset, alreadyCharged),
          const SizedBox(height: 20),
          Text(
            'What it cost, and what is left',
            style: theme.textTheme.titleMedium,
          ),
          const SizedBox(height: 8),
          _line(context, 'Bought for', asset.cost.toDecimalString()),
          _line(
            context,
            'Written off so far',
            asset.writtenOff.toDecimalString(),
          ),
          _line(
            context,
            'Worth now',
            asset.netBookValue.toDecimalString(),
            strong: true,
          ),
          if (asset.salvageValue > ExactAmount.zero)
            _line(
              context,
              'Expected at the end',
              asset.salvageValue.toDecimalString(),
            ),
          const SizedBox(height: 20),
          Text('The months charged', style: theme.textTheme.titleMedium),
          const SizedBox(height: 8),
          if (_history.isEmpty)
            Text(
              'Nothing has been charged yet.',
              style: theme.textTheme.bodyMedium,
            )
          else
            for (final charge in _history) _chargeRow(context, charge),
          const SizedBox(height: 24),
          if (asset.isHeld)
            OutlinedButton.icon(
              onPressed: _dispose,
              icon: const Icon(Icons.sell_outlined),
              label: const Text('Record that it is gone'),
            ),
          const SizedBox(height: 12),
          Text(
            'Writing an asset off is not the same as paying for it. The '
            'depreciation entry belongs in Accounting, where the month is '
            'closed.',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }

  Widget _standingCard(BuildContext context, FixedAsset asset) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Worth now', style: theme.textTheme.labelLarge),
            const SizedBox(height: 4),
            Text(
              asset.netBookValue.toDecimalString(),
              style: theme.textTheme.headlineMedium,
            ),
            const SizedBox(height: 8),
            Text(
              [
                asset.category.label,
                if (asset.code.isNotEmpty) asset.code,
                if (asset.acquiredOn.isNotEmpty) 'bought ${asset.acquiredOn}',
                if (asset.usefulLifeYears > 0)
                  'used for ${asset.usefulLifeYears} '
                      '${asset.usefulLifeYears == 1 ? 'year' : 'years'}',
                asset.method.label.toLowerCase(),
                // the rate an asset was registered with is the one the register charges by, so it is said back here
                // rather than left for someone to infer from a month's figure
                if (asset.method == DepreciationMethod.reducingBalance &&
                    !asset.rate.isZero)
                  '${asset.rate.toDecimalString().replaceFirst(RegExp(r'\.0+$'), '')}% a year',
              ].join(' · '),
              style: theme.textTheme.bodyMedium,
            ),
            if (asset.location.isNotEmpty || asset.department.isNotEmpty) ...[
              const SizedBox(height: 4),
              Text(
                [
                  if (asset.location.isNotEmpty) asset.location,
                  if (asset.department.isNotEmpty) asset.department,
                ].join(' · '),
                style: theme.textTheme.bodySmall,
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _chargeCard(
    BuildContext context,
    FixedAsset asset,
    bool alreadyCharged,
  ) {
    final theme = Theme.of(context);
    // a month the asset was not held for is one the engine refuses, so it is said here rather than offered
    final refusal = asset.refusalFor(_month);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('This month', style: theme.textTheme.titleMedium),
            const SizedBox(height: 6),
            Text(
              alreadyCharged
                  ? '$_month has already been charged. The next month to charge '
                        'is ${FixedAsset.nextPeriod(_month)}.'
                  : refusal ??
                        '$_month would take '
                            '${asset.chargeFor(_month).toDecimalString()} off '
                            'what it is worth.',
              style: theme.textTheme.bodyMedium,
            ),
            const SizedBox(height: 12),
            FilledButton.icon(
              onPressed: alreadyCharged || refusal != null ? null : _charge,
              icon: const Icon(Icons.event_available_outlined),
              label: Text(
                alreadyCharged
                    ? '$_month is charged'
                    : 'Charge ${asset.chargeFor(_month).toDecimalString()} for '
                          '$_month',
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _line(
    BuildContext context,
    String label,
    String amount, {
    bool strong = false,
  }) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Expanded(
            child: Text(
              label,
              style: strong
                  ? theme.textTheme.bodyLarge?.copyWith(
                      fontWeight: FontWeight.w600,
                    )
                  : theme.textTheme.bodyMedium,
            ),
          ),
          Text(
            amount,
            style: strong
                ? theme.textTheme.bodyLarge?.copyWith(
                    fontWeight: FontWeight.w600,
                  )
                : theme.textTheme.bodyMedium,
          ),
        ],
      ),
    );
  }

  Widget _chargeRow(BuildContext context, DepreciationCharge charge) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(charge.period, style: theme.textTheme.bodyMedium),
                Text(
                  'left ${charge.netBookValue.toDecimalString()}',
                  style: theme.textTheme.bodySmall,
                ),
              ],
            ),
          ),
          Text(
            charge.amount.toDecimalString(),
            style: theme.textTheme.bodyMedium,
          ),
        ],
      ),
    );
  }

  Widget _note(BuildContext context, String words) {
    final theme = Theme.of(context);
    return Card(
      color: theme.colorScheme.surfaceContainerHighest,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Text(words, style: theme.textTheme.bodyMedium),
      ),
    );
  }
}

/// Recording that a thing is gone: sold, scrapped, or given away.
class DisposeAssetSheet extends StatefulWidget {
  const DisposeAssetSheet({
    super.key,
    required this.service,
    required this.asset,
    required this.today,
  });

  final FixedAssetsService service;
  final FixedAsset asset;
  final String today;

  static Future<bool?> show(
    BuildContext context, {
    required FixedAssetsService service,
    required FixedAsset asset,
    required String today,
  }) {
    return showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      builder: (_) =>
          DisposeAssetSheet(service: service, asset: asset, today: today),
    );
  }

  @override
  State<DisposeAssetSheet> createState() => _DisposeAssetSheetState();
}

class _DisposeAssetSheetState extends State<DisposeAssetSheet> {
  final _proceeds = TextEditingController(text: '0');
  final _notes = TextEditingController();
  late final TextEditingController _date = TextEditingController(
    text: '${widget.today}-01',
  );
  String _method = 'sale';
  bool _saving = false;
  String? _problem;

  @override
  void dispose() {
    _proceeds.dispose();
    _notes.dispose();
    _date.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final proceeds = ExactAmount.tryParse(_proceeds.text);
    if (proceeds == null) {
      setState(() => _problem = 'Type what it fetched as a figure, or 0.');
      return;
    }
    setState(() {
      _saving = true;
      _problem = null;
    });
    try {
      await widget.service.dispose(
        id: widget.asset.id,
        disposalDate: _date.text,
        proceeds: proceeds,
        method: _method,
        notes: _notes.text,
      );
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _problem = CouldNotRead.reasonOf(failure);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final gainOrLoss =
        (ExactAmount.tryParse(_proceeds.text) ?? ExactAmount.zero) -
        widget.asset.netBookValue;
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
            Text('Record that it is gone', style: theme.textTheme.titleLarge),
            const SizedBox(height: 4),
            Text(
              'It is worth ${widget.asset.netBookValue.toDecimalString()} in the '
              'books today.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _proceeds,
                    keyboardType: const TextInputType.numberWithOptions(
                      decimal: true,
                    ),
                    decoration: const InputDecoration(
                      labelText: 'What it fetched',
                      hintText: '0',
                    ),
                    onChanged: (_) => setState(() {}),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: TextField(
                    controller: _date,
                    decoration: const InputDecoration(labelText: 'On'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              initialValue: _method,
              decoration: const InputDecoration(labelText: 'How it went'),
              items: const [
                DropdownMenuItem(value: 'sale', child: Text('Sold')),
                DropdownMenuItem(value: 'scrap', child: Text('Scrapped')),
                DropdownMenuItem(value: 'donation', child: Text('Given away')),
              ],
              onChanged: (picked) {
                if (picked != null) setState(() => _method = picked);
              },
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _notes,
              decoration: const InputDecoration(
                labelText: 'Anything to note (optional)',
              ),
            ),
            const SizedBox(height: 12),
            Text(
              gainOrLoss.isNegative
                  ? 'That is ${gainOrLoss.negated().toDecimalString()} less than '
                        'it is worth, so the books will show a loss.'
                  : 'That is ${gainOrLoss.toDecimalString()} more than it is '
                        'worth, so the books will show a gain.',
              style: theme.textTheme.bodyMedium,
            ),
            if (_problem != null) ...[
              const SizedBox(height: 12),
              Text(
                _problem!,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.error,
                ),
              ),
            ],
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _saving ? null : () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: _saving ? null : _save,
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
