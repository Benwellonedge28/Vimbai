import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';

/// Choose an account from the Book's chart, by its number or its name.
///
/// A Book opens with the chart it takes to write in it, which means the numbers exist before the person has any reason
/// to know them. Asking for one blind - the form's account field - turns the first entry into a lookup of codes the
/// person has never seen. The chart in front of them is the fix: numbers and names, searchable, and typing a number
/// still works for anyone who knows it.
Future<Account?> showAccountPicker(
  BuildContext context, {
  required List<Account> accounts,
}) {
  return showModalBottomSheet<Account>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (context) => _AccountPicker(accounts: accounts),
  );
}

class _AccountPicker extends StatefulWidget {
  const _AccountPicker({required this.accounts});

  final List<Account> accounts;

  @override
  State<_AccountPicker> createState() => _AccountPickerState();
}

class _AccountPickerState extends State<_AccountPicker> {
  String _query = '';

  List<Account> _matched() {
    final query = _query.trim().toLowerCase();
    if (query.isEmpty) return widget.accounts;
    return widget.accounts
        .where(
          (account) =>
              account.accountNumber.toLowerCase().contains(query) ||
              account.accountName.toLowerCase().contains(query) ||
              account.accountType.toLowerCase().contains(query),
        )
        .toList();
  }

  @override
  Widget build(BuildContext context) {
    final matched = _matched();
    return SafeArea(
      child: Padding(
        padding: EdgeInsets.only(
          left: 16,
          right: 16,
          bottom: MediaQuery.of(context).viewInsets.bottom + 8,
        ),
        child: SizedBox(
          height: MediaQuery.of(context).size.height * 0.6,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Text(
                'Choose an account',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
              ),
              const SizedBox(height: 8),
              TextField(
                decoration: const InputDecoration(
                  hintText: 'Search by number, name or type',
                  prefixIcon: Icon(Icons.search),
                  isDense: true,
                  border: OutlineInputBorder(),
                ),
                onChanged: (value) => setState(() => _query = value),
              ),
              const SizedBox(height: 8),
              Expanded(
                child: widget.accounts.isEmpty
                    ? const Center(
                        child: Padding(
                          padding: EdgeInsets.all(16),
                          child: Text(
                            'This Book has no accounts yet. You can still type a number.',
                            textAlign: TextAlign.center,
                          ),
                        ),
                      )
                    : matched.isEmpty
                    ? const Center(child: Text('No account matches that.'))
                    : ListView.builder(
                        itemCount: matched.length,
                        itemBuilder: (context, index) {
                          final account = matched[index];
                          return ListTile(
                            dense: true,
                            title: Text(
                              '${account.accountNumber} - ${account.accountName}',
                            ),
                            subtitle: Text(account.accountType),
                            onTap: () => Navigator.of(context).pop(account),
                          );
                        },
                      ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
