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
                              ' (${org.orgType == 'commercial' ? 'business' : 'non-profit'})'
                              ' - revenue ${org.annualRevenue.toStringAsFixed(0)} USD',
                            ),
                            trailing: PopupMenuButton<String>(
                              onSelected: (v) {
                                if (v == 'donation') _addDonation(org);
                                if (v == 'sale') _addRevenue(org);
                                if (v == 'reports') _showReports(org);
                              },
                              itemBuilder: (ctx) => [
                                if (org.orgType == 'commercial')
                                  const PopupMenuItem(
                                    value: 'sale',
                                    child: Text('Record sale'),
                                  )
                                else
                                  const PopupMenuItem(
                                    value: 'donation',
                                    child: Text('Record donation'),
                                  ),
                                const PopupMenuItem(
                                  value: 'reports',
                                  child: Text('View reports'),
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
