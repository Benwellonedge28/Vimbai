import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/finance_api_service.dart';
import 'package:vimbai_mobile_client/models/finance_models.dart';
import 'package:intl/intl.dart';

class BudgetFormPage extends StatefulWidget {
  const BudgetFormPage({super.key});

  @override
  State<BudgetFormPage> createState() => _BudgetFormPageState();
}

class _BudgetFormPageState extends State<BudgetFormPage> {
  final _formKey = GlobalKey<FormState>();
  final TextEditingController _nameController = TextEditingController();
  final TextEditingController _descriptionController = TextEditingController();
  final TextEditingController _currencyController = TextEditingController(text: 'USD');

  DateTime _startDate = DateTime.now();
  DateTime _endDate = DateTime.now().add(const Duration(days: 365));

  List<BudgetItemFormData> _budgetItems = [];
  final FinanceApiService _apiService = FinanceApiService();

  void _addBudgetItem() {
    setState(() {
      _budgetItems.add(BudgetItemFormData());
    });
  }

  void _removeBudgetItem(int index) {
    setState(() {
      _budgetItems.removeAt(index);
    });
  }

  Future<void> _selectStartDate() async {
    final DateTime? picked = await showDatePicker(
      context: context,
      initialDate: _startDate,
      firstDate: DateTime(2000),
      lastDate: DateTime(2101),
    );
    if (picked != null) {
      setState(() {
        _startDate = picked;
        if (_endDate.isBefore(_startDate)) {
          _endDate = _startDate.add(const Duration(days: 365));
        }
      });
    }
  }

  Future<void> _selectEndDate() async {
    final DateTime? picked = await showDatePicker(
      context: context,
      initialDate: _endDate,
      firstDate: _startDate,
      lastDate: DateTime(2101),
    );
    if (picked != null) {
      setState(() {
        _endDate = picked;
      });
    }
  }

  Future<void> _submitForm() async {
    if (_formKey.currentState!.validate()) {
      if (_budgetItems.isEmpty) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Please add at least one budget item.')),
        );
        return;
      }

      final items = _budgetItems.map((item) => BudgetItem(
        category: item.category,
        accountNumber: item.accountNumber,
        budgetedAmount: item.budgetedAmount,
        budgetType: item.budgetType,
        createdAt: DateTime.now(),
        updatedAt: DateTime.now(),
      )).toList();

      try {
        await _apiService.createBudget(
          BudgetCreate(
            name: _nameController.text,
            startDate: _startDate,
            endDate: _endDate,
            currency: _currencyController.text,
            description: _descriptionController.text.isEmpty ? null : _descriptionController.text,
          ),
        );
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(content: Text('Budget created successfully!')),
          );
          Navigator.of(context).pop();
        }
      } catch (e) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Error creating budget: ${e.toString()}')),
          );
        }
      }
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
            crossAxisAlignment: CrossAxisAlignment.stretch,
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
              const SizedBox(height: 16),
              Row(
                children: [
                  Expanded(
                    child: InkWell(
                      onTap: _selectStartDate,
                      child: InputDecorator(
                        decoration: const InputDecoration(labelText: 'Start Date'),
                        child: Text(DateFormat('yyyy-MM-dd').format(_startDate)),
                      ),
                    ),
                  ),
                  const SizedBox(width: 16),
                  Expanded(
                    child: InkWell(
                      onTap: _selectEndDate,
                      child: InputDecorator(
                        decoration: const InputDecoration(labelText: 'End Date'),
                        child: Text(DateFormat('yyyy-MM-dd').format(_endDate)),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 16),
              TextFormField(
                controller: _currencyController,
                decoration: const InputDecoration(labelText: 'Currency'),
                validator: (value) {
                  if (value == null || value.isEmpty) {
                    return 'Please enter a currency';
                  }
                  return null;
                },
              ),
              const SizedBox(height: 16),
              TextFormField(
                controller: _descriptionController,
                decoration: const InputDecoration(labelText: 'Description (Optional)'),
                maxLines: 3,
              ),
              const SizedBox(height: 24),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  const Text('Budget Items', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
                  ElevatedButton.icon(
                    onPressed: _addBudgetItem,
                    icon: const Icon(Icons.add),
                    label: const Text('Add Item'),
                  ),
                ],
              ),
              const SizedBox(height: 16),
              ..._budgetItems.asMap().entries.map((entry) {
                int idx = entry.key;
                BudgetItemFormData item = entry.value;
                // ignore: unused_local_variable
                final _ = item;
                return Card(
                  margin: const EdgeInsets.symmetric(vertical: 8),
                  child: Padding(
                    padding: const EdgeInsets.all(12.0),
                    child: Column(
                      children: [
                        TextFormField(
                          decoration: const InputDecoration(labelText: 'Category'),
                          onChanged: (value) {
                            setState(() {
                              _budgetItems[idx].category = value;
                            });
                          },
                          validator: (value) {
                            if (value == null || value.isEmpty) {
                              return 'Required';
                            }
                            return null;
                          },
                        ),
                        const SizedBox(height: 12),
                        TextFormField(
                          decoration: const InputDecoration(labelText: 'Budgeted Amount'),
                          keyboardType: TextInputType.number,
                          onChanged: (value) {
                            setState(() {
                              _budgetItems[idx].budgetedAmount = double.tryParse(value) ?? 0.0;
                            });
                          },
                          validator: (value) {
                            if (value == null || double.tryParse(value) == null || double.parse(value) < 0) {
                              return 'Enter a valid amount';
                            }
                            return null;
                          },
                        ),
                        const SizedBox(height: 12),
                        TextFormField(
                          decoration: const InputDecoration(labelText: 'Account Number (Optional)'),
                          onChanged: (value) {
                            setState(() {
                              _budgetItems[idx].accountNumber = value;
                            });
                          },
                        ),
                        const SizedBox(height: 12),
                        DropdownButtonFormField<String>(
                          initialValue: _budgetItems[idx].budgetType,
                          decoration: const InputDecoration(labelText: 'Budget Type'),
                          items: <String>['expense', 'revenue', 'asset', 'liability'].map((String value) {
                            return DropdownMenuItem<String>(
                              value: value,
                              child: Text(value),
                            );
                          }).toList(),
                          onChanged: (String? newValue) {
                            if (newValue != null) {
                              setState(() {
                                _budgetItems[idx].budgetType = newValue;
                              });
                            }
                          },
                        ),
                        const SizedBox(height: 8),
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
              const SizedBox(height: 24),
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

class BudgetItemFormData {
  String category;
  double budgetedAmount;
  String accountNumber;
  String budgetType;

  BudgetItemFormData({
    this.category = '',
    this.budgetedAmount = 0.0,
    this.accountNumber = '',
    this.budgetType = 'expense',
  });
}