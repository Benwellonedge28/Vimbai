// Family & community savings groups (mukando-style round): groups,
// members, per-cycle contributions and payout schedules. Backed by
// family-community-group-service, scoped to the active Book.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/services/group_savings_service.dart';

class GroupSavingsPage extends StatefulWidget {
  const GroupSavingsPage({super.key});

  @override
  State<GroupSavingsPage> createState() => _GroupSavingsPageState();
}

class _GroupSavingsPageState extends State<GroupSavingsPage> {
  final GroupSavingsService _gs = GroupSavingsService.instance;

  List<Map<String, dynamic>> _groups = [];
  bool _loading = true;
  String? _error;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    setState(() { _loading = true; _error = null; });
    try {
      final groups = await _gs.listGroups();
      if (mounted) setState(() => _groups = groups);
    } catch (e) {
      if (mounted) setState(() => _error = 'Could not load groups: $e');
    }
    if (mounted) setState(() => _loading = false);
  }

  void _toast(String msg) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));

  Future<void> _createGroup() async {
    final name = TextEditingController();
    final desc = TextEditingController();
    final amt = TextEditingController();
    String freq = 'monthly';
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setD) => AlertDialog(
          title: const Text('New savings group'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(controller: name, decoration: const InputDecoration(labelText: 'Group name')),
              const SizedBox(height: 8),
              TextField(controller: desc, decoration: const InputDecoration(labelText: 'Description (optional)')),
              const SizedBox(height: 8),
              TextField(
                controller: amt,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(labelText: 'Contribution amount'),
              ),
              const SizedBox(height: 8),
              DropdownButton<String>(
                value: freq,
                isExpanded: true,
                items: const [
                  DropdownMenuItem(value: 'weekly', child: Text('Weekly')),
                  DropdownMenuItem(value: 'biweekly', child: Text('Every 2 weeks')),
                  DropdownMenuItem(value: 'monthly', child: Text('Monthly')),
                ],
                onChanged: (v) => setD(() => freq = v ?? freq),
              ),
            ],
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
            ElevatedButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Create')),
          ],
        ),
      ),
    );
    if (ok != true) return;
    final amount = double.tryParse(amt.text.trim()) ?? 0;
    if (name.text.trim().isEmpty || amount <= 0) {
      _toast('Enter a group name and a positive contribution amount');
      return;
    }
    try {
      await _gs.createGroup(
        name: name.text.trim(),
        description: desc.text.trim(),
        contributionFrequency: freq,
        contributionAmount: amount,
      );
      _toast('Group created');
      _reload();
    } catch (e) {
      _toast('Create failed: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Group savings')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _createGroup,
        icon: const Icon(Icons.add),
        label: const Text('New group'),
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : RefreshIndicator(
              onRefresh: _reload,
              child: _error != null
                  ? ListView(
                      padding: const EdgeInsets.all(16),
                      children: [
                        Text(_error!),
                        TextButton(onPressed: _reload, child: const Text('Retry')),
                      ],
                    )
                  : _groups.isEmpty
                      ? ListView(
                          padding: const EdgeInsets.all(24),
                          children: const [
                            Icon(Icons.groups_outlined, size: 48),
                            SizedBox(height: 12),
                            Text(
                              'No savings groups yet. Create your first group to start tracking contributions, cycles and payouts.',
                              textAlign: TextAlign.center,
                            ),
                          ],
                        )
                      : ListView.builder(
                          padding: const EdgeInsets.all(12),
                          itemCount: _groups.length,
                          itemBuilder: (ctx, i) {
                            final g = _groups[i];
                            return Card(
                              child: ListTile(
                                leading: const Icon(Icons.groups),
                                title: Text('${g['name']}'),
                                subtitle: Text(
                                  'Cycle ${g['current_cycle']} - pool ${g['total_pool']} - '
                                  '${g['member_count']} members - ${g['contribution_frequency']}',
                                ),
                                trailing: const Icon(Icons.chevron_right),
                                onTap: () => Navigator.of(context).push(
                                  MaterialPageRoute(
                                    builder: (ctx) => GroupDetailPage(groupId: '${g['id']}'),
                                  ),
                                ).then((_) => _reload()),
                              ),
                            );
                          },
                        ),
            ),
    );
  }
}

class GroupDetailPage extends StatefulWidget {
  final String groupId;
  const GroupDetailPage({super.key, required this.groupId});

  @override
  State<GroupDetailPage> createState() => _GroupDetailPageState();
}

class _GroupDetailPageState extends State<GroupDetailPage>
    with SingleTickerProviderStateMixin {
  final GroupSavingsService _gs = GroupSavingsService.instance;
  late final TabController _tabs = TabController(length: 3, vsync: this);
  Map<String, dynamic>? _group;
  List<Map<String, dynamic>> _members = [];
  List<Map<String, dynamic>> _contributions = [];
  List<Map<String, dynamic>> _payouts = [];
  String? _error;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  @override
  void dispose() {
    _tabs.dispose();
    super.dispose();
  }

  Future<void> _reload() async {
    try {
      final g = await _gs.getGroup(widget.groupId);
      final ms = await _gs.listMembers(widget.groupId);
      final cs = await _gs.listContributions(widget.groupId);
      final ps = await _gs.listPayouts(widget.groupId);
      if (mounted) {
        setState(() {
          _group = g;
          _members = ms;
          _contributions = cs;
          _payouts = ps;
          _error = null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  void _toast(String msg) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));

  Future<void> _addMember() async {
    final name = TextEditingController();
    final phone = TextEditingController();
    final amt = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Add member'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(controller: name, decoration: const InputDecoration(labelText: 'Member name')),
            const SizedBox(height: 8),
            TextField(controller: phone, decoration: const InputDecoration(labelText: 'Phone (optional)')),
            const SizedBox(height: 8),
            TextField(
              controller: amt,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Contribution amount (optional)'),
            ),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
          ElevatedButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Add')),
        ],
      ),
    );
    if (ok != true) return;
    if (name.text.trim().isEmpty) {
      _toast('Enter a member name');
      return;
    }
    try {
      await _gs.addMember(
        widget.groupId,
        name: name.text.trim(),
        phone: phone.text.trim(),
        contributionAmount: double.tryParse(amt.text.trim()) ?? 0.0,
      );
      _toast('Member added');
      _reload();
    } catch (e) {
      _toast('Add failed: $e');
    }
  }

  Future<void> _recordContribution() async {
    if (_members.isEmpty) {
      _toast('Add members first');
      return;
    }
    String? memberId = '${_members.first['id']}';
    final amt = TextEditingController();
    final notes = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setD) => AlertDialog(
          title: const Text('Record contribution'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              DropdownButton<String>(
                value: memberId,
                isExpanded: true,
                items: _members
                    .map((m) => DropdownMenuItem(value: '${m['id']}', child: Text('${m['name']}')))
                    .toList(),
                onChanged: (v) => setD(() => memberId = v),
              ),
              const SizedBox(height: 8),
              TextField(
                controller: amt,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(labelText: 'Amount'),
              ),
              const SizedBox(height: 8),
              TextField(controller: notes, decoration: const InputDecoration(labelText: 'Notes (optional)')),
            ],
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
            ElevatedButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Record')),
          ],
        ),
      ),
    );
    if (ok != true) return;
    final amount = double.tryParse(amt.text.trim()) ?? 0;
    if (amount <= 0 || memberId == null) {
      _toast('Enter a positive amount');
      return;
    }
    try {
      await _gs.contribute(
        widget.groupId,
        memberId: memberId,
        amount: amount,
        notes: notes.text.trim(),
      );
      _toast('Contribution recorded');
      _reload();
    } catch (e) {
      _toast('Record failed: $e');
    }
  }

  Future<void> _advanceCycle() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Advance cycle?'),
        content: Text(
          'Move "${_group?['name']}" from cycle ${_group?['current_cycle']} to '
          'cycle ${(int.tryParse('${_group?['current_cycle']}') ?? 1) + 1}? '
          'New contributions will count toward the new cycle.',
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
          ElevatedButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Advance')),
        ],
      ),
    );
    if (ok != true) return;
    try {
      final r = await _gs.advanceCycle(widget.groupId);
      _toast('Now in cycle ${r['current_cycle']}');
      _reload();
    } catch (e) {
      _toast('Advance failed: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final g = _group;
    return Scaffold(
      appBar: AppBar(title: Text(g == null ? 'Group' : '${g['name']}')),
      floatingActionButton: AnimatedBuilder(
        animation: _tabs,
        builder: (ctx, _) => _tabs.index == 0
            ? FloatingActionButton.extended(
                onPressed: _addMember,
                icon: const Icon(Icons.person_add),
                label: const Text('Member'),
              )
            : _tabs.index == 1
                ? FloatingActionButton.extended(
                    onPressed: _recordContribution,
                    icon: const Icon(Icons.payments),
                    label: const Text('Contribute'),
                  )
                : const SizedBox.shrink(),
      ),
      body: _error != null
          ? Center(child: Text(_error!))
          : g == null
              ? const Center(child: CircularProgressIndicator())
              : Column(
                  children: [
                    Card(
                      margin: const EdgeInsets.all(12),
                      child: Padding(
                        padding: const EdgeInsets.all(12),
                        child: Row(
                          children: [
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('Cycle ${g['current_cycle']}', style: theme.textTheme.titleMedium),
                                  Text(
                                    'Pool: ${g['total_pool']} - ${g['member_count']} members',
                                    style: theme.textTheme.bodySmall,
                                  ),
                                ],
                              ),
                            ),
                            ElevatedButton.icon(
                              onPressed: _advanceCycle,
                              icon: const Icon(Icons.skip_next),
                              label: const Text('Advance cycle'),
                            ),
                          ],
                        ),
                      ),
                    ),
                    Expanded(
                      child: Column(
                          children: [
                            TabBar(
                              controller: _tabs,
                              tabs: [
                                Tab(icon: Icon(Icons.people_outline), text: 'Members'),
                                Tab(icon: Icon(Icons.payments_outlined), text: 'Contributions'),
                                Tab(icon: Icon(Icons.event_outlined), text: 'Payouts'),
                              ],
                            ),
                            Expanded(
                              child: TabBarView(
                                controller: _tabs,
                                children: [
                                  RefreshIndicator(
                                    onRefresh: _reload,
                                    child: ListView.builder(
                                      itemCount: _members.length,
                                      itemBuilder: (ctx, i) {
                                        final m = _members[i];
                                        return ListTile(
                                          leading: const Icon(Icons.person_outline),
                                          title: Text('${m['name']}'),
                                          subtitle: Text(
                                            'contribution: ${m['contribution_amount']}'
                                            '${m['phone'].toString().isNotEmpty ? ' - ${m['phone']}' : ''}',
                                          ),
                                        );
                                      },
                                    ),
                                  ),
                                  RefreshIndicator(
                                    onRefresh: _reload,
                                    child: ListView.builder(
                                      itemCount: _contributions.length,
                                      itemBuilder: (ctx, i) {
                                        final c = _contributions[i];
                                        final member = _members
                                            .where((m) => '${m['id']}' == '${c['member_id']}')
                                            .map((m) => '${m['name']}')
                                        .followedBy(['member'])
                                            .first;
                                        return ListTile(
                                          leading: const Icon(Icons.payments),
                                          title: Text('$member - ${c['amount']}'),
                                          subtitle: Text(
                                            'cycle ${c['cycle_number']}'
                                            '${c['notes'].toString().isNotEmpty ? ' - ${c['notes']}' : ''}',
                                          ),
                                        );
                                      },
                                    ),
                                  ),
                                  RefreshIndicator(
                                    onRefresh: _reload,
                                    child: ListView.builder(
                                      itemCount: _payouts.length,
                                      itemBuilder: (ctx, i) {
                                        final p = _payouts[i];
                                        return ListTile(
                                          leading: const Icon(Icons.event_outlined),
                                          title: Text('${p['payout_amount']} - cycle ${p['cycle_number']}'),
                                          subtitle: Text('${p['status']}'),
                                        );
                                      },
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
    );
  }
}
