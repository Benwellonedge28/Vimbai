import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/services/finance_api_service.dart';
import 'package:finacc_mobile_client/models/finance_models.dart';
import 'package:intl/intl.dart';

class BudgetFormPage extends StatefulWidget {
  const BudgetFormPage({super.key});

  @override
  State<BudgetFormPage> createState() => _BudgetFormPageState();
}

class _BudgetFormPageState extends State<BudgetFormPage> {
  final _formKey = GlobalKey<FormState>();
  final TextEditingController _nameController = TextEditingController();
  final TextEditingController _fiscalYearController = TextEditingController();
  final TextEditingController _periodController = TextEditingController();
  final TextEditingController _descriptionController = TextEditingController();
  String _status = 'Draft';

  List<BudgetItem> _budgetItems = [];
  final FinanceApiService _apiService = FinanceApiService();

  void _addBudgetItem() {
    setState(() {
      _budgetItems.add(BudgetItem(
        category: '',
        budgetedAmount: 0.0,
        periodStart: DateTime.now(),
        periodEnd: DateTime.now().add(const Duration(days: 30)),
      ));
    });
  }

  void _removeBudgetItem(int index) {
    setState(() {
      _budgetItems.removeAt(index);
    });
  }

  Future<void> _selectDate(BuildContext context, int index, bool isStart) async {
    final DateTime? picked = await showDatePicker(
      context: context,
      initialDate: isStart ? _budgetItems[index].periodStart : _budgetItems[index].periodEnd,
      firstDate: DateTime(2000),
      lastDate: DateTime(2101),
    );
    if (picked != null) {
      setState(() {
        final currentItem = _budgetItems[index];
        _budgetItems[index] = BudgetItem(
          id: currentItem.id,
          category: currentItem.category,
          budgetedAmount: currentItem.budgetedAmount,
          actualAmount: currentItem.actualAmount,
          description: currentItem.description,
          accountNumber: currentItem.accountNumber,
          periodStart: isStart ? picked : currentItem.periodStart,
          periodEnd: isStart ? currentItem.periodEnd : picked,
          createdAt: currentItem.createdAt,
          updatedAt: currentItem.updatedAt,
        );
      });
    }
  }

  Future<void> _submitForm() async {
    if (_formKey.currentState!.validate() && _budgetItems.isNotEmpty) {
      final newBudget = Budget(
        name: _nameController.text,
        fiscalYear: int.parse(_fiscalYearController.text),
        period: _periodController.text,
        description: _descriptionController.text.isEmpty ? null : _descriptionController.text,
        status: _status,
        items: _budgetItems,
      );

      try {
        await _apiService.createBudget(newBudget);
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Budget created successfully!')),
          );
          Navigator.of(context).pop(); // Go back to previous screen
        }
      } catch (e) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Error creating budget: ${e.toString()}')),
          );
        }
      }
    } else {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Please ensure the form is valid and has at least one budget item.')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Create New Budget'),
          ),
          body: Form(
            key: _formKey,
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                children: [
                  TextFormField(
                    controller: _nameController,
                    decoration: const InputDecoration(labelText: 'Budget Name'),
                    validator: (value) {
                      if (value == null || value.isEmpty) {
                        return 'Please enter a budget name';
                      }
                      return null;
                    },
                  ),
                  TextFormField(
                    controller: _fiscalYearController,
                    decoration: const InputDecoration(labelText: 'Fiscal Year'),
                    keyboardType: TextInputType.number,
                    validator: (value) {
                      if (value == null || int.tryParse(value) == null) {
                        return 'Please enter a valid fiscal year';
                      }
                      return null;
                    },
                  ),
                  TextFormField(
                    controller: _periodController,
                    decoration: const InputDecoration(labelText: 'Period (e.g., Q1, Month 1)'),
                    validator: (value) {
                      if (value == null || value.isEmpty) {
                        return 'Please enter a period';
                      }
                      return null;
                    },
                  ),
                  TextFormField(
                    controller: _descriptionController,
                    decoration: const InputDecoration(labelText: 'Description (Optional)'),
                    maxLines: 3,
                  ),
                  DropdownButtonFormField<String>(
                    value: _status,
                    decoration: const InputDecoration(labelText: 'Status'),
                    items: <String>['Draft', 'Approved', 'Closed'].map((String value) {
                      return DropdownMenuItem<String>(
                        value: value,
                        child: Text(value),
                      );
                    }).toList(),
                    onChanged: (String? newValue) {
                      if (newValue != null) {
                        setState(() {
                          _status = newValue;
                        });
                      }
                    },
                  ),
                  const SizedBox(height: 20),
                  const Text('Budget Items', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                  ..._budgetItems.asMap().entries.map((entry) {
                    int idx = entry.key;
                    BudgetItem item = entry.value;
                    return Card(
                      margin: const EdgeInsets.symmetric(vertical: 8),
                      child: Padding(
                        padding: const EdgeInsets.all(12.0),
                        child: Column(
                          children: [
                            TextFormField(
                              initialValue: item.category,
                              decoration: const InputDecoration(labelText: 'Category'),
                              onChanged: (value) => item = BudgetItem(
                                  category: value,
                                  budgetedAmount: item.budgetedAmount,
                                  periodStart: item.periodStart,
                                  periodEnd: item.periodEnd,
                                  description: item.description,
                                  accountNumber: item.accountNumber),
                              validator: (value) {
                                if (value == null || value.isEmpty) {
                                  return 'Required';
                                }
                                return null;
                              },
                            ),
                            TextFormField(
                              initialValue: item.budgetedAmount.toStringAsFixed(2),
                              decoration: const InputDecoration(labelText: 'Budgeted Amount'),
                              keyboardType: TextInputType.number,
                              onChanged: (value) {
                                final amount = double.tryParse(value) ?? 0.0;
                                setState(() {
                                  _budgetItems[idx] = BudgetItem(
                                      category: item.category,
                                      budgetedAmount: amount,
                                      periodStart: item.periodStart,
                                      periodEnd: item.periodEnd,
                                      description: item.description,
                                      accountNumber: item.accountNumber);
                                });
                              },
                              validator: (value) {
                                if (value == null || double.tryParse(value) == null || double.parse(value) < 0) {
                                  return 'Enter a valid amount';
                                }
                                return null;
                              },
                            ),
                            TextFormField(
                              initialValue: item.description,
                              decoration: const InputDecoration(labelText: 'Description (Optional)'),
                              onChanged: (value) => item = BudgetItem(
                                  category: item.category,
                                  budgetedAmount: item.budgetedAmount,
                                  periodStart: item.periodStart,
                                  periodEnd: item.periodEnd,
                                  description: value,
                                  accountNumber: item.accountNumber),
                            ),
                            TextFormField(
                              initialValue: item.accountNumber,
                              decoration: const InputDecoration(labelText: 'Account Number (Optional)'),
                              onChanged: (value) => item = BudgetItem(
                                  category: item.category,
                                  budgetedAmount: item.budgetedAmount,
                                  periodStart: item.periodStart,
                                  periodEnd: item.periodEnd,
                                  description: item.description,
                                  accountNumber: value),
                            ),
                            Row(
                              children: [
                                Expanded(
                                  child: InkWell(
                                    onTap: () => _selectDate(context, idx, true),
                                    child: InputDecorator(
                                      decoration: const InputDecoration(labelText: 'Period Start'),
                                      baseStyle: const TextStyle(fontSize: 16),
                                      child: Text(DateFormat('yyyy-MM-dd').format(item.periodStart)),
                                    ),
                                  ),
                                ),
                                const SizedBox(width: 10),
                                Expanded(
                                  child: InkWell(
                                    onTap: () => _selectDate(context, idx, false),
                                    child: InputDecorator(
                                      decoration: const InputDecoration(labelText: 'Period End'),
                                      baseStyle: const TextStyle(fontSize: 16),
                                      child: Text(DateFormat('yyyy-MM-dd').format(item.periodEnd)),
                                    ),
                                  ),
                                ),
                              ],
                            ),
                            Align(
                              alignment: Alignment.centerRight,
                              child: IconButton(
                                icon: const Icon(Icons.delete, color: Colors.red),
                                onPressed: () => _removeBudgetItem(idx),
                              ),
                            ),
                          ],
                        ),
                      ),
                    );
                  }).toList(),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: _addBudgetItem,
                    child: const Text('Add Budget Item'),
                  ),
                  const SizedBox(height: 20),
                  ElevatedButton(
                    onPressed: _submitForm,
                    child: const Text('Create Budget'),
                  ),
                ],
              ),
            ),
          ),
        );
      }
    }
