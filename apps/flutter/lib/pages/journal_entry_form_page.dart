import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/models/accounting_models.dart';
import 'package:connectivity_plus/connectivity_plus.dart'; // NEW
import 'package:uuid/uuid.dart'; // NEW for local UUID

class JournalEntryFormPage extends StatefulWidget {
  const JournalEntryFormPage({super.key});

  @override
  State<JournalEntryFormPage> createState() => _JournalEntryFormPageState();
}

class _JournalEntryFormPageState extends State<JournalEntryFormPage> {
  final _formKey = GlobalKey<FormState>();
  final TextEditingController _descriptionController = TextEditingController();
  final TextEditingController _referenceNumberController = TextEditingController();

  List<JournalLine> _journalLines = [];
  final AccountingApiService _apiService = AccountingApiService();
  final Uuid _uuid = Uuid(); // NEW

  ConnectivityResult _connectivityResult = ConnectivityResult.none; // NEW

  @override
  void initState() {
    super.initState();
    _checkConnectivity(); // NEW
    Connectivity().onConnectivityChanged.listen((List<ConnectivityResult> results) { // NEW
      setState(() {
        _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
      });
    });
  }

  Future<void> _checkConnectivity() async { // NEW
    final results = await Connectivity().checkConnectivity();
    _connectivityResult = results.isEmpty ? ConnectivityResult.none : results.last;
    setState(() {});
  }

  void _addJournalLine() {
    setState(() {
      _journalLines.add(JournalLine(accountNumber: '', debit: 0.0, credit: 0.0));
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

  Future<void> _submitForm() async {
    if (_formKey.currentState!.validate() && _journalLines.length >= 2 && _isBalanced()) {
      final newEntry = JournalEntry(
        id: _uuid.v4(), // NEW: Assign a local UUID
        entryDate: DateTime.now(),
        description: _descriptionController.text,
        referenceNumber: _referenceNumberController.text.isEmpty ? null : _referenceNumberController.text,
        sourceModule: 'MobileApp',
        lines: _journalLines,
      );

      try {
        if (_connectivityResult == ConnectivityResult.none) { // NEW: Handle offline
          await _apiService.createJournalEntry(newEntry, isOffline: true);
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              const SnackBar(content: Text('Journal Entry saved offline. Will sync when online!')),
            );
            Navigator.of(context).pop();
          }
        } else { // Online
          await _apiService.createJournalEntry(newEntry);
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              const SnackBar(content: Text('Journal Entry created successfully!')),
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
      String errorMessage = 'Please ensure the form is valid, has at least two lines, and is balanced.';
      if (_journalLines.length < 2) {
        errorMessage = 'A journal entry must have at least two lines.';
      }
      else if (!_isBalanced()) {
        errorMessage = 'Debits must equal Credits.';
      }
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(errorMessage)),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Create Journal Entry'),
          ),
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
                    decoration: const InputDecoration(labelText: 'Reference Number (Optional)'),
                  ),
                  const SizedBox(height: 20),
                  const Text('Journal Lines', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
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
                              initialValue: line.accountNumber,
                              decoration: const InputDecoration(labelText: 'Account Number'),
                              onChanged: (value) {
                                setState(() {
                                  _journalLines[idx] = JournalLine(accountNumber: value, debit: line.debit, credit: line.credit, description: line.description);
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
                                  _journalLines[idx] = JournalLine(accountNumber: line.accountNumber, debit: debit, credit: 0.0, description: line.description);
                                });
                              },
                            ),
                            TextFormField(
                              initialValue: line.credit.toStringAsFixed(2),
                              decoration: const InputDecoration(labelText: 'Credit'),
                              keyboardType: TextInputType.number,
                              onChanged: (value) {
                                final credit = double.tryParse(value) ?? 0.0;
                                setState(() {
                                  _journalLines[idx] = JournalLine(accountNumber: line.accountNumber, debit: 0.0, credit: credit, description: line.description);
                                });
                              },
                            ),
                            TextFormField(
                              initialValue: line.description,
                              decoration: const InputDecoration(labelText: 'Line Description (Optional)'),
                              onChanged: (value) {
                                setState(() {
                                  _journalLines[idx] = JournalLine(accountNumber: line.accountNumber, debit: line.debit, credit: line.credit, description: value);
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
                      Text('Total Debits: ${_calculateTotalDebits().toStringAsFixed(2)}'),
                      Text('Total Credits: ${_calculateTotalCredits().toStringAsFixed(2)}'),
                    ],
                  ),
                  const SizedBox(height: 10),
                  Text(
                    _isBalanced() ? 'Entry is Balanced' : 'Entry is NOT Balanced',
                    style: TextStyle(color: _isBalanced() ? Colors.green : Colors.red),
                  ),
                  const SizedBox(height: 20),
                  ElevatedButton(
                    onPressed: _submitForm,
                    child: Text(_connectivityResult == ConnectivityResult.none ? 'Save Offline' : 'Submit Journal Entry'), // NEW label
                  ),
                  Text('Connectivity: ${_connectivityResult == ConnectivityResult.none ? 'Offline' : 'Online'}'), // NEW status indicator
                ],
              ),
            ),
          ),
        );
      }
    }
