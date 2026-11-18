// The things the business owns: what they cost, and what is left of that cost.
//
// A business buys a vehicle once and uses it for years. The profit and loss shows the month's depreciation, the balance
// sheet shows what is left, and neither of them tells a person which asset is which. This page is the register itself: it
// leads with what everything still held is worth now, then each thing with what it cost and what it is worth, and lets a
// person add what was bought or open one to charge its month.
//
// A disposed asset stays on the list marked as gone. Selling a van is part of the register's history, and hiding it would
// make the page disagree with the year it reports on.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_asset_register.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_assets_service.dart';
import 'package:vimbai_mobile_client/features/assets/presentation/fixed_asset_page.dart';

class FixedAssetsPage extends StatefulWidget {
  const FixedAssetsPage({super.key, this.service, this.today});

  /// A test passes its own service so the page can be exercised without a server.
  final FixedAssetsService? service;

  /// A test fixes today, so an asset bought today is dated by the test and not by the clock.
  final String? today;

  @override
  State<FixedAssetsPage> createState() => _FixedAssetsPageState();
}

class _FixedAssetsPageState extends State<FixedAssetsPage> {
  late final FixedAssetsService _assets =
      widget.service ?? FixedAssetsService.instance;

  late final String _today =
      widget.today ?? DateTime.now().toIso8601String().substring(0, 10);

  bool _loading = true;
  String? _error;
  AssetRegister? _register;

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
      final register = await _assets.register();
      if (!mounted) return;
      setState(() {
        _register = register;
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

  Future<void> _buy() async {
    final saved = await BuyAssetSheet.show(
      context,
      service: _assets,
      today: _today,
    );
    if (saved == true) await _load();
  }

  Future<void> _open(FixedAsset asset) async {
    final changed = await Navigator.of(context).push<bool>(
      MaterialPageRoute(
        builder: (_) => FixedAssetPage(assetId: asset.id, service: _assets),
      ),
    );
    if (changed == true) await _load();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Things you own')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _buy,
        icon: const Icon(Icons.add),
        label: const Text('Record a purchase'),
      ),
      body: _body(context),
    );
  }

  Widget _body(BuildContext context) {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return CouldNotRead(
        what: 'the register of things you own',
        error: _error,
        onRetry: _load,
      );
    }
    final register = _register;
    if (register == null || register.isEmpty) {
      return _nothingYet(context);
    }
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 96),
        children: [
          _worthCard(context, register),
          if (register.withoutALife.isNotEmpty) ...[
            const SizedBox(height: 12),
            _noLifeNote(context, register),
          ],
          const SizedBox(height: 20),
          Text('What you hold', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          for (final asset in register.assets) _assetTile(context, asset),
          if (register.byCategory.isNotEmpty) ...[
            const SizedBox(height: 24),
            Text('By kind', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            for (final total in register.byCategory)
              _categoryRow(context, total),
          ],
        ],
      ),
    );
  }

  Widget _nothingYet(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.inventory_2_outlined,
              size: 48,
              color: Theme.of(context).colorScheme.primary,
            ),
            const SizedBox(height: 16),
            Text(
              'Nothing is on the register yet',
              style: Theme.of(context).textTheme.titleMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            Text(
              'Record a vehicle, a machine or a computer and the register will '
              'write it off a month at a time, so what the business is worth '
              'stays honest.',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodyMedium,
            ),
            const SizedBox(height: 20),
            FilledButton.icon(
              onPressed: _buy,
              icon: const Icon(Icons.add),
              label: const Text('Record a purchase'),
            ),
          ],
        ),
      ),
    );
  }

  Widget _worthCard(BuildContext context, AssetRegister register) {
    final theme = Theme.of(context);
    final writtenOff = register.writtenOff;
    final cost = register.costOfWhatIsHeld;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Worth now', style: theme.textTheme.labelLarge),
            const SizedBox(height: 4),
            Text(
              register.worthNow.toDecimalString(),
              style: theme.textTheme.headlineMedium,
            ),
            const SizedBox(height: 8),
            Text(
              '${register.held.length} '
              '${register.held.length == 1 ? 'thing' : 'things'} held, bought '
              'for ${cost.toDecimalString()}, with '
              '${writtenOff.toDecimalString()} written off so far.',
              style: theme.textTheme.bodyMedium,
            ),
            if (register.assets.length > register.held.length) ...[
              const SizedBox(height: 6),
              Text(
                '${register.assets.length - register.held.length} more on the '
                'register have been sold or scrapped, and are listed below.',
                style: theme.textTheme.bodySmall,
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _noLifeNote(BuildContext context, AssetRegister register) {
    final theme = Theme.of(context);
    final names = register.withoutALife
        .map((asset) => asset.name.isEmpty ? asset.code : asset.name)
        .join(', ');
    return Card(
      color: theme.colorScheme.errorContainer,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(Icons.info_outline, color: theme.colorScheme.onErrorContainer),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                'These have no useful life recorded, so nothing is being '
                'written off them: $names. Open each one and give it a life.',
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onErrorContainer,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _assetTile(BuildContext context, FixedAsset asset) {
    final theme = Theme.of(context);
    final gone = !asset.isHeld;
    final name = asset.name.isEmpty ? asset.code : asset.name;
    return Card(
      margin: const EdgeInsets.only(bottom: 8),
      child: ListTile(
        onTap: () => _open(asset),
        title: Text(
          name,
          style: gone
              ? theme.textTheme.titleMedium?.copyWith(
                  decoration: TextDecoration.lineThrough,
                )
              : theme.textTheme.titleMedium,
        ),
        subtitle: Text(
          [
            asset.category.label,
            if (asset.code.isNotEmpty) asset.code,
            if (asset.acquiredOn.isNotEmpty) 'bought ${asset.acquiredOn}',
            if (gone) 'sold or scrapped',
          ].join(' · '),
        ),
        trailing: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Text(
              asset.netBookValue.toDecimalString(),
              style: theme.textTheme.titleMedium,
            ),
            Text(
              'of ${asset.cost.toDecimalString()}',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ),
      ),
    );
  }

  Widget _categoryRow(BuildContext context, AssetCategoryTotal total) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          Expanded(
            child: Text(
              '${total.category.label} (${total.count})',
              style: theme.textTheme.bodyMedium,
            ),
          ),
          Text(
            total.worth.toDecimalString(),
            style: theme.textTheme.bodyMedium,
          ),
        ],
      ),
    );
  }
}

/// Recording something the business has bought and will use for years.
class BuyAssetSheet extends StatefulWidget {
  const BuyAssetSheet({super.key, required this.service, required this.today});

  final FixedAssetsService service;
  final String today;

  /// Opens the sheet and answers true when something was recorded.
  static Future<bool?> show(
    BuildContext context, {
    required FixedAssetsService service,
    required String today,
  }) {
    return showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      builder: (_) => BuyAssetSheet(service: service, today: today),
    );
  }

  @override
  State<BuyAssetSheet> createState() => _BuyAssetSheetState();
}

class _BuyAssetSheetState extends State<BuyAssetSheet> {
  final _code = TextEditingController();
  final _name = TextEditingController();
  final _cost = TextEditingController();
  final _life = TextEditingController();
  final _salvage = TextEditingController();
  final _location = TextEditingController();

  AssetCategory _category = AssetCategory.equipment;
  DepreciationMethod _method = DepreciationMethod.straightLine;
  late final TextEditingController _date = TextEditingController(
    text: widget.today,
  );

  bool _saving = false;
  String? _problem;

  @override
  void dispose() {
    for (final controller in [
      _code,
      _name,
      _cost,
      _life,
      _salvage,
      _location,
      _date,
    ]) {
      controller.dispose();
    }
    super.dispose();
  }

  Future<void> _save() async {
    final cost = ExactAmount.tryParse(_cost.text);
    if (cost == null) {
      setState(() => _problem = 'Type the cost as a figure, like 4500.00.');
      return;
    }
    final salvage = _salvage.text.trim().isEmpty
        ? ExactAmount.zero
        : ExactAmount.tryParse(_salvage.text);
    if (salvage == null) {
      setState(
        () => _problem = 'Type what it will be worth at the end as a figure.',
      );
      return;
    }
    final life = int.tryParse(_life.text.trim());
    if (life == null) {
      setState(() => _problem = 'Type the years it will be used as a number.');
      return;
    }
    setState(() {
      _saving = true;
      _problem = null;
    });
    try {
      await widget.service.buy(
        code: _code.text,
        name: _name.text,
        category: _category,
        cost: cost,
        usefulLifeYears: life,
        salvageValue: salvage,
        acquiredOn: _date.text,
        method: _method,
        location: _location.text,
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
            Text('Record a purchase', style: theme.textTheme.titleLarge),
            const SizedBox(height: 4),
            Text(
              'The register writes it off over the years you will use it, a '
              'month at a time.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _name,
              decoration: const InputDecoration(
                labelText: 'What it is',
                hintText: 'Delivery van',
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _code,
              decoration: const InputDecoration(
                labelText: 'Code',
                hintText: 'VEH-001',
              ),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<AssetCategory>(
              value: _category,
              decoration: const InputDecoration(labelText: 'Kind'),
              items: [
                for (final category in AssetCategory.values)
                  DropdownMenuItem(
                    value: category,
                    child: Text(category.label),
                  ),
              ],
              onChanged: (picked) {
                if (picked != null) setState(() => _category = picked);
              },
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _cost,
                    keyboardType: const TextInputType.numberWithOptions(
                      decimal: true,
                    ),
                    decoration: const InputDecoration(
                      labelText: 'What it cost',
                      hintText: '4500.00',
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: TextField(
                    controller: _salvage,
                    keyboardType: const TextInputType.numberWithOptions(
                      decimal: true,
                    ),
                    decoration: const InputDecoration(
                      labelText: 'Worth at the end',
                      hintText: '500.00',
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _life,
                    keyboardType: TextInputType.number,
                    decoration: const InputDecoration(
                      labelText: 'Years it will be used',
                      hintText: '5',
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: TextField(
                    controller: _date,
                    decoration: const InputDecoration(
                      labelText: 'Bought on',
                      hintText: '2026-11-18',
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<DepreciationMethod>(
              value: _method,
              decoration: const InputDecoration(labelText: 'Written off'),
              items: [
                for (final method in DepreciationMethod.values)
                  DropdownMenuItem(value: method, child: Text(method.label)),
              ],
              onChanged: (picked) {
                if (picked != null) setState(() => _method = picked);
              },
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _location,
              decoration: const InputDecoration(
                labelText: 'Where it is kept (optional)',
                hintText: 'Head office',
              ),
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
