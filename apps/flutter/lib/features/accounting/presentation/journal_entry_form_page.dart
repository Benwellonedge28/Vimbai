import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/models/journal_entry_drafting.dart';
import 'package:connectivity_plus/connectivity_plus.dart'; // NEW
import 'package:uuid/uuid.dart'; // NEW for local UUID
import 'package:vimbai_mobile_client/features/accounting/presentation/account_picker_sheet.dart';

class JournalEntryFormPage extends StatefulWidget {
  const JournalEntryFormPage({super.key, this.service});

  /// The engine the form talks to. A test hands it one that answers without a phone.
  final AccountingApiService? service;

  @override
  State<JournalEntryFormPage> createState() => _JournalEntryFormPageState();
}

class _JournalEntryFormPageState extends State<JournalEntryFormPage> {
  final _formKey = GlobalKey<FormState>();
  final TextEditingController _descriptionController = TextEditingController();
  final TextEditingController _referenceNumberController =
      TextEditingController();

  List<JournalLine> _journalLines = [];
  late final AccountingApiService _apiService =
      widget.service ?? AccountingApiService();

  /// The Book's chart, so a line can be given an account by name instead of by remembered number. An empty list is not
  /// a fault to report here: typing a number stays possible, and the cashier's flow must not stall because a chart read
  /// failed.
  List<Account> _accounts = const [];
  final Uuid _uuid = Uuid(); // NEW

  ConnectivityResult _connectivityResult = ConnectivityResult.none; // NEW

  @override
  void initState() {
    super.initState();
    _checkConnectivity(); // NEW
    _loadChart();
    Connectivity().onConnectivityChanged.listen((
      List<ConnectivityResult> results,
    ) {
      // NEW
      setState(() {
        _connectivityResult = results.isEmpty
            ? ConnectivityResult.none
            : results.last;
      });
    });
  }

  Future<void> _checkConnectivity() async {
    // NEW
    final results = await Connectivity().checkConnectivity();
    _connectivityResult = results.isEmpty
        ? ConnectivityResult.none
        : results.last;
    setState(() {});
  }

  Future<void> _loadChart() async {
    try {
      final accounts = await _apiService.getAllAccounts();
      if (mounted) setState(() => _accounts = accounts);
    } catch (_) {
      // the field stays free text
    }
  }

  /// The name of the account a line names, when the chart knows it - the number alone does not tell a person whether
  /// they chose the bank or the bank loan.
  String? _nameOf(String number) {
    for (final account in _accounts) {
      if (account.accountNumber == number.trim()) return account.accountName;
    }
    return null;
  }

  Future<void> _chooseAccount(int index) async {
    final line = _journalLines[index];
    final chosen = await showAccountPicker(context, accounts: _accounts);
    if (chosen == null || !mounted) return;
    setState(() {
      _journalLines[index] = JournalLine(
        accountNumber: chosen.accountNumber,
        debit: line.debit,
        credit: line.credit,
        description: line.description,
      );
    });
  }

  void _addJournalLine() {
    setState(() {
      _journalLines.add(
        JournalLine(accountNumber: '', debit: 0.0, credit: 0.0),
      );
    });
  }

  void _removeJournalLine(int index) {
    setState(() {
      _journalLines.removeAt(index);
    });
  }

  double _calculateTotalDebits() {
    return _journalLines.fold(0.0, (sum, line) => sum + line.debit);
  }

  double _calculateTotalCredits() {
    return _journalLines.fold(0.0, (sum, line) => sum + line.credit);
  }

  bool _isBalanced() {
    return _calculateTotalDebits() == _calculateTotalCredits();
  }

  /// Keep this entry as a draft: it is held, and the statements do not count it until it is posted.
  ///
  /// The same rules apply as posting - two lines that balance - because a draft that could not be posted later would be a
  /// record of nothing. What changes is that it is out of the statements until somebody posts it.
  Future<void> _saveAsDraft() async {
    if (!(_formKey.currentState!.validate() &&
        _journalLines.length >= 2 &&
        _isBalanced())) {
      return;
    }
    final draft = buildJournalEntry(
      id: _uuid.v4(),
      entryDate: DateTime.now(),
      description: _descriptionController.text,
      referenceNumber: _referenceNumberController.text,
      lines: _journalLines,
      intent: JournalEntryIntent.keepAsDraft,
    );
    try {
      await _apiService.createJournalEntry(draft);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text(
            'Kept as a draft. It counts in the statements once posted.',
          ),
        ),
      );
      Navigator.of(context).pop();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Could not keep the draft: $e')));
    }
  }

  Future<void> _submitForm() async {
    if (_formKey.currentState!.validate() &&
        _journalLines.length >= 2 &&
        _isBalanced()) {
      final newEntry = buildJournalEntry(
        id: _uuid.v4(), // NEW: Assign a local UUID
        entryDate: DateTime.now(),
        description: _descriptionController.text,
        referenceNumber: _referenceNumberController.text,
        lines: _journalLines,
        intent: JournalEntryIntent.post,
      );

      try {
        if (_connectivityResult == ConnectivityResult.none) {
          // NEW: Handle offline
          await _apiService.createJournalEntry(newEntry, isOffline: true);
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              const SnackBar(
                content: Text(
                  'Journal Entry saved offline. Will sync when online!',
                ),
              ),
            );
            Navigator.of(context).pop();
          }
        } else {
          // Online
          await _apiService.createJournalEntry(newEntry);
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              const SnackBar(
                content: Text('Journal Entry created successfully!'),
              ),
            );
            Navigator.of(context).pop();
          }
        }
      } catch (e) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Error creating entry: ${e.toString()}')),
          );
        }
      }
    } else {
      String errorMessage =
          'Please ensure the form is valid, has at least two lines, and is balanced.';
      if (_journalLines.length < 2) {
        errorMessage = 'A journal entry must have at least two lines.';
      } else if (!_isBalanced()) {
        errorMessage = 'Debits must equal Credits.';
      }
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(errorMessage)));
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Create Journal Entry')),
      body: Form(
        key: _formKey,
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            children: [
              // ... (existing form fields) ...
              TextFormField(
                controller: _descriptionController,
                decoration: const InputDecoration(labelText: 'Description'),
                validator: (value) {
                  if (value == null || value.isEmpty) {
                    return 'Please enter a description';
                  }
                  return null;
                },
              ),
              TextFormField(
                controller: _referenceNumberController,
                decoration: const InputDecoration(
                  labelText: 'Reference Number (Optional)',
                ),
              ),
              const SizedBox(height: 20),
              const Text(
                'Journal Lines',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
              ),
              ..._journalLines.asMap().entries.map((entry) {
                int idx = entry.key;
                JournalLine line = entry.value;
                return Card(
                  margin: const EdgeInsets.symmetric(vertical: 8),
                  child: Padding(
                    padding: const EdgeInsets.all(12.0),
                    child: Column(
                      children: [
                        TextFormField(
                          key: ValueKey('account-$idx-${line.accountNumber}'),
                          initialValue: line.accountNumber,
                          decoration: InputDecoration(
                            labelText: 'Account Number',
                            helperText: _nameOf(line.accountNumber),
                            suffixIcon: IconButton(
                              icon: const Icon(Icons.list_alt),
                              tooltip: 'Choose an account',
                              onPressed: () => _chooseAccount(idx),
                            ),
                          ),
                          onChanged: (value) {
                            setState(() {
                              _journalLines[idx] = JournalLine(
                                accountNumber: value,
                                debit: line.debit,
                                credit: line.credit,
                                description: line.description,
                              );
                            });
                          },
                          validator: (value) {
                            if (value == null || value.isEmpty) {
                              return 'Required';
                            }
                            return null;
                          },
                        ),
                        TextFormField(
                          initialValue: line.debit.toStringAsFixed(2),
                          decoration: const InputDecoration(labelText: 'Debit'),
                          keyboardType: TextInputType.number,
                          onChanged: (value) {
                            final debit = double.tryParse(value) ?? 0.0;
                            setState(() {
                              _journalLines[idx] = JournalLine(
                                accountNumber: line.accountNumber,
                                debit: debit,
                                credit: 0.0,
                                description: line.description,
                              );
                            });
                          },
                        ),
                        TextFormField(
                          initialValue: line.credit.toStringAsFixed(2),
                          decoration: const InputDecoration(
                            labelText: 'Credit',
                          ),
                          keyboardType: TextInputType.number,
                          onChanged: (value) {
                            final credit = double.tryParse(value) ?? 0.0;
                            setState(() {
                              _journalLines[idx] = JournalLine(
                                accountNumber: line.accountNumber,
                                debit: 0.0,
                                credit: credit,
                                description: line.description,
                              );
                            });
                          },
                        ),
                        TextFormField(
                          initialValue: line.description,
                          decoration: const InputDecoration(
                            labelText: 'Line Description (Optional)',
                          ),
                          onChanged: (value) {
                            setState(() {
                              _journalLines[idx] = JournalLine(
                                accountNumber: line.accountNumber,
                                debit: line.debit,
                                credit: line.credit,
                                description: value,
                              );
                            });
                          },
                        ),
                        Align(
                          alignment: Alignment.centerRight,
                          child: IconButton(
                            icon: const Icon(Icons.delete, color: Colors.red),
                            onPressed: () => _removeJournalLine(idx),
                          ),
                        ),
                      ],
                    ),
                  ),
                );
              }).toList(),
              const SizedBox(height: 10),
              ElevatedButton(
                onPressed: _addJournalLine,
                child: const Text('Add Journal Line'),
              ),
              const SizedBox(height: 20),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(
                    'Total Debits: ${_calculateTotalDebits().toStringAsFixed(2)}',
                  ),
                  Text(
                    'Total Credits: ${_calculateTotalCredits().toStringAsFixed(2)}',
                  ),
                ],
              ),
              const SizedBox(height: 10),
              Text(
                _isBalanced() ? 'Entry is Balanced' : 'Entry is NOT Balanced',
                style: TextStyle(
                  color: _isBalanced() ? Colors.green : Colors.red,
                ),
              ),
              const SizedBox(height: 20),
              ElevatedButton(
                onPressed: _submitForm,
                child: Text(
                  _connectivityResult == ConnectivityResult.none
                      ? 'Save Offline'
                      : 'Submit Journal Entry',
                ), // NEW label
              ),
              // An entry that is not ready to post still deserves to be kept: the engine holds a draft and keeps it out of
              // the statements until it is posted, so half-finished work no longer means starting again from a blank form.
              if (_connectivityResult != ConnectivityResult.none)
                TextButton(
                  onPressed: _saveAsDraft,
                  child: const Text('Save as draft'),
                ),
              Text(
                'Connectivity: ${_connectivityResult == ConnectivityResult.none ? 'Offline' : 'Online'}',
              ), // NEW status indicator
            ],
          ),
        ),
      ),
    );
  }
}
