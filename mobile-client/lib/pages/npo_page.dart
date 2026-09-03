// NPO page - non-profit organizations, small to extra-large.
//
// Shows the user's non-profit organizations, the size band each is in
// (small / medium / large / extra_large) and the unlocked features,
// and gives a quick path to record a donation. All donor-grade
// reports (statement of activities, financial position) are one tap
// away - the kind of transparency donors and regulators expect.

import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/services/npo_scale_service.dart';

class NpoPage extends StatefulWidget {
  const NpoPage({super.key});

  @override
  State<NpoPage> createState() => _NpoPageState();
}

class _NpoPageState extends State<NpoPage> {
  final NpoScaleService _npo = NpoScaleService.instance;
  List<NpoOrg> _orgs = [];
  bool _loading = true;
  String? _error;
  String? _selectedOrgId;

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
      final orgs = await _npo.myOrgs();
      if (!mounted) return;
      setState(() {
        _orgs = orgs;
        if (_selectedOrgId == null && orgs.isNotEmpty) {
          _selectedOrgId = orgs.first.id;
        }
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = 'Could not load organizations - are you offline?';
      });
    }
  }

  Future<void> _createOrg() async {
    final nameCtrl = TextEditingController();
    final revenueCtrl = TextEditingController();
    String orgType = 'nonprofit';
    final created = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) {
        return StatefulBuilder(
          builder: (ctx, setDialog) => AlertDialog(
            title: const Text('New organization'),
            content: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  controller: nameCtrl,
                  decoration: const InputDecoration(
                    labelText: 'Organization name',
                  ),
                ),
                RadioListTile<String>(
                  value: 'nonprofit',
                  groupValue: orgType,
                  onChanged: (v) => setDialog(() => orgType = v ?? orgType),
                  title: const Text('Non-profit'),
                ),
                RadioListTile<String>(
                  value: 'commercial',
                  groupValue: orgType,
                  onChanged: (v) => setDialog(() => orgType = v ?? orgType),
                  title: const Text('Business'),
                ),
                RadioListTile<String>(
                  value: 'partnership',
                  groupValue: orgType,
                  onChanged: (v) => setDialog(() => orgType = v ?? orgType),
                  title: const Text('Partnership'),
                ),
                RadioListTile<String>(
                  value: 'company',
                  groupValue: orgType,
                  onChanged: (v) => setDialog(() => orgType = v ?? orgType),
                  title: const Text('Private company'),
                ),
                RadioListTile<String>(
                  value: 'plc',
                  groupValue: orgType,
                  onChanged: (v) => setDialog(() => orgType = v ?? orgType),
                  title: const Text('Public company (PLC)'),
                ),
                TextField(
                  controller: revenueCtrl,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(
                    labelText: 'Annual revenue (USD, optional)',
                  ),
                ),
              ],
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(ctx),
                child: const Text('Cancel'),
              ),
              ElevatedButton(
                onPressed: () => Navigator.pop(ctx, {
                  'name': nameCtrl.text.trim(),
                  'revenue': revenueCtrl.text.trim(),
                  'org_type': orgType,
                }),
                child: const Text('Create'),
              ),
            ],
          ),
        );
      },
    );
    if (created == null || (created['name'] ?? '').isEmpty) return;
    try {
      final revenue =
          double.tryParse(created['revenue'] ?? '') ?? 0;
      await _npo.createOrg(
        created['name']!,
        orgType: created['org_type'] ?? 'nonprofit',
        annualRevenue: revenue,
      );
      await _load();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Create failed: $e')),
        );
      }
    }
  }

  Future<void> _addRevenue(NpoOrg org) async {
    final amountCtrl = TextEditingController();
    final customerCtrl = TextEditingController();
    final result = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Record sale'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: customerCtrl,
              decoration: const InputDecoration(labelText: 'Customer'),
            ),
            TextField(
              controller: amountCtrl,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Amount (USD)'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(ctx, {
              'customer': customerCtrl.text.trim(),
              'amount': amountCtrl.text.trim(),
            }),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (result == null) return;
    final amount = double.tryParse(result['amount'] ?? '') ?? 0;
    if (amount <= 0) return;
    try {
      final resp = await _npo.addRevenue(
        org.id,
        amount,
        customer: result['customer'] ?? '',
      );
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Receipt ${resp['receipt_no']} issued'),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Sale failed: $e')),
        );
      }
    }
  }

  Future<void> _addPurchase(NpoOrg org) async {
    final vendorCtrl = TextEditingController();
    final descCtrl = TextEditingController();
    final amountCtrl = TextEditingController();
    final result = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Record purchase (on credit)'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: vendorCtrl,
              decoration: const InputDecoration(labelText: 'Vendor name'),
            ),
            TextField(
              controller: descCtrl,
              decoration: const InputDecoration(labelText: 'What was bought'),
            ),
            TextField(
              controller: amountCtrl,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Amount (USD)'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(ctx, {
              'vendor': vendorCtrl.text.trim(),
              'desc': descCtrl.text.trim(),
              'amount': amountCtrl.text.trim(),
            }),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (result == null) return;
    final amount = double.tryParse(result['amount'] ?? '') ?? 0;
    if ((result['vendor'] ?? '').isEmpty || amount <= 0) return;
    try {
      final vendorId = await _npo.addVendor(org.id, result['vendor']!);
      await _npo.recordPurchase(
        org.id,
        vendorId,
        result['desc'] ?? '',
        amount,
      );
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Purchase recorded - it is now owed to the vendor'),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Purchase failed: $e')),
        );
      }
    }
  }

  Future<void> _showCreditors(NpoOrg org) async {
    try {
      final r = await _npo.creditorsReport(org.id);
      final creditors = (r['creditors'] as List<dynamic>).cast<Map>();
      if (!mounted) return;
      await showDialog(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text('${org.name} - creditors'),
          content: creditors.isEmpty
              ? const Text('Nothing owed to vendors.')
              : Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    for (final c in creditors)
                      Text('${c['vendor']}: owed ${c['owed']} USD'),
                    const SizedBox(height: 8),
                    Text(
                      'Total owed: ${r['total_owed']} USD',
                      style: const TextStyle(fontWeight: FontWeight.bold),
                    ),
                  ],
                ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Creditors failed: $e')),
        );
      }
    }
  }

  Future<void> _showShareholders(NpoOrg org) async {
    try {
      final holders = await _npo.getShareholders(org.id);
      if (!mounted) return;
      await showDialog(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text('${org.name} - shareholders'),
          content: holders.isEmpty
              ? const Text('No shareholders yet.')
              : SizedBox(
                  width: 320,
                  child: SingleChildScrollView(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        for (final h in holders)
                          Padding(
                            padding: const EdgeInsets.only(bottom: 10),
                            child: Text(
                              '${h['name']}: ${h['shares']} shares '
                              '(${h['amount_paid']} paid in)\n'
                              'Investor verify code: ${h['verify_code'] ?? 'not issued'}',
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Shareholders failed: $e')),
        );
      }
    }
  }

  Future<void> _addDonation(NpoOrg org) async {
    final nameCtrl = TextEditingController();
    final amountCtrl = TextEditingController();
    final result = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Record donation'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: nameCtrl,
              decoration: const InputDecoration(labelText: 'Donor name'),
            ),
            TextField(
              controller: amountCtrl,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Amount (USD)'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(ctx, {
              'donor': nameCtrl.text.trim(),
              'amount': amountCtrl.text.trim(),
            }),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (result == null) return;
    final amount = double.tryParse(result['amount'] ?? '') ?? 0;
    if ((result['donor'] ?? '').isEmpty || amount <= 0) return;
    try {
      final donorId = await _npo.addDonor(org.id, result['donor']!, 'individual');
      final resp = await _npo.addDonation(org.id, donorId, amount);
      if (mounted) {
        final receipt = resp['receipt'] as Map<String, dynamic>;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              'Receipt ${receipt['receipt_no']} issued',
            ),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Donation failed: $e')),
        );
      }
    }
  }

  Future<void> _showReports(NpoOrg org) async {
    try {
      final pos = await _npo.positionReport(org.id);
      final act = await _npo.activitiesReport(org.id);
      if (!mounted) return;
      await showDialog(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text('${org.name} - reports'),
          content: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text(
                  'Financial position',
                  style: TextStyle(fontWeight: FontWeight.bold),
                ),
                Text('Assets: ${pos['assets']}'),
                Text('Liabilities: ${pos['liabilities']}'),
                Text('Net assets: ${pos['net_assets']}'),
                const SizedBox(height: 12),
                const Text(
                  'Statement of activities',
                  style: TextStyle(fontWeight: FontWeight.bold),
                ),
                for (final f in (act['funds'] as List<dynamic>))
                  Text(
                    '${(f as Map)['fund']}: net ${(f)['net']}',
                  ),
                const SizedBox(height: 12),
                Text('Total net: ${act['total_net']}'),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Reports failed: $e')),
        );
      }
    }
  }

  static const Map<String, String> _typeLabels = {
    'nonprofit': 'non-profit',
    'commercial': 'business',
    'partnership': 'partnership',
    'company': 'private company',
    'plc': 'public company (PLC)',
  };

  static const Map<String, String> _bandLabels = {
    'sole_trader': 'Sole trader',
    'small': 'Small (community trust / business)',
    'medium': 'Medium (single office)',
    'large': 'Large (multi-branch)',
    'extra_large': 'Extra-large (federation / group)',
  };

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Non-profit organizations'),
        actions: [
          IconButton(
            icon: const Icon(Icons.add),
            tooltip: 'New organization',
            onPressed: _createOrg,
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : RefreshIndicator(
              onRefresh: _load,
              child: _orgs.isEmpty
                  ? ListView(
                      children: const [
                        Padding(
                          padding: EdgeInsets.all(24),
                          child: Text(
                            'No organizations yet. Create one for your '
                            'spaza shop, company, trust, church or NGO - '
                            'Vimbai scales from a sole trader to a '
                            'national federation.',
                          ),
                        ),
                      ],
                    )
                  : ListView.builder(
                      itemCount: _orgs.length,
                      itemBuilder: (ctx, i) {
                        final org = _orgs[i];
                        return Card(
                          child: ListTile(
                            leading: const Icon(Icons.volunteer_activism),
                            title: Text(org.name),
                            subtitle: Text(
                              '${_bandLabels[org.sizeBand] ?? org.sizeBand}'
                              ' (${_typeLabels[org.orgType] ?? org.orgType})'
                              ' - revenue ${org.annualRevenue.toStringAsFixed(0)} USD',
                            ),
                            trailing: PopupMenuButton<String>(
                              onSelected: (v) {
                                if (v == 'donation') _addDonation(org);
                                if (v == 'sale') _addRevenue(org);
                                if (v == 'purchase') _addPurchase(org);
                                if (v == 'reports') _showReports(org);
                                if (v == 'creditors') _showCreditors(org);
                                if (v == 'shareholders') {
                                  _showShareholders(org);
                                }
                              },
                              itemBuilder: (ctx) => [
                                if (org.orgType == 'commercial')
                                  const PopupMenuItem(
                                    value: 'sale',
                                    child: Text('Record sale'),
                                  )
                                else if (org.orgType == 'partnership' ||
                                    org.orgType == 'company' ||
                                    org.orgType == 'plc')
                                  const PopupMenuItem(
                                    value: 'sale',
                                    child: Text('Record revenue'),
                                  )
                                else
                                  const PopupMenuItem(
                                    value: 'donation',
                                    child: Text('Record donation'),
                                  ),
                                const PopupMenuItem(
                                  value: 'purchase',
                                  child: Text('Record purchase'),
                                ),
                                const PopupMenuItem(
                                  value: 'reports',
                                  child: Text('View reports'),
                                ),
                                const PopupMenuItem(
                                  value: 'creditors',
                                  child: Text('Creditors (owed)'),
                                ),
                                if (org.orgType == 'company' ||
                                    org.orgType == 'plc')
                                  const PopupMenuItem(
                                    value: 'shareholders',
                                    child: Text('Shareholders'),
                                  ),
                              ],
                            ),
                          ),
                        );
                      },
                    ),
            ),
    );
  }
}
