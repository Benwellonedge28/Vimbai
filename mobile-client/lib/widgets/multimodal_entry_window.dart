// mobile-client/lib/widgets/multimodal_entry_window.dart

import 'package:flutter/material.dart';
import 'package:finacc_mobile_client/models/multimodal_models.dart'; // To use DocumentParseResult, AudioParseResult etc.
import 'package:finacc_mobile_client/services/multimodal_api_service.dart'; // To interact with the multimodal service
import 'package:finacc_mobile_client/services/accounting_api_service.dart'; // To create/update journal entries
import 'package:finacc_mobile_client/models/accounting_models.dart'; // For JournalEntryCreate, JournalLineCreate
import 'package:decimal/decimal.dart'; // For Decimal type

class MultimodalEntryWindow extends StatefulWidget {
  final MultimodalInput? initialInput; // Could be an offline-queued task or newly received input
  final DocumentParseResult? initialDocumentResult;
  final AudioParseResult? initialAudioResult;

  const MultimodalEntryWindow({
    Key? key,
    this.initialInput,
    this.initialDocumentResult,
    this.initialAudioResult,
  }) : super(key: key);

  @override
  _MultimodalEntryWindowState createState() => _MultimodalEntryWindowState();
}

class _MultimodalEntryWindowState extends State<MultimodalEntryWindow> {
  final _formKey = GlobalKey<FormState>();
  late TextEditingController _descriptionController;
  late TextEditingController _amountController;
  late TextEditingController _debitAccountController;
  late TextEditingController _creditAccountController;
  late DateTime _entryDate;
  String? _extractedText; // For OCR/ASR output
  List<ExtractedDataField> _extractedFields = []; // For structured data from OCR

  bool _isProcessing = false;
  String? _errorMessage;
  String? _successMessage;

  @override
  void initState() {
    super.initState();
    _descriptionController = TextEditingController();
    _amountController = TextEditingController();
    _debitAccountController = TextEditingController();
    _creditAccountController = TextEditingController();
    _entryDate = DateTime.now();

    _initializeFromProps();
  }

  void _initializeFromProps() {
    if (widget.initialDocumentResult != null) {
      _extractedText = widget.initialDocumentResult!.rawText;
      _extractedFields = widget.initialDocumentResult!.extractedData;
      _descriptionController.text = _findExtractedValue('description') ?? '';
      _amountController.text = _findExtractedValue('total_amount') ?? '';
      _entryDate = DateTime.tryParse(_findExtractedValue('date') ?? '') ?? DateTime.now();
      // Pre-fill accounts based on AI suggestions or user preferences if available
      _debitAccountController.text = _findExtractedValue('suggested_debit_account') ?? '';
      _creditAccountController.text = _findExtractedValue('suggested_credit_account') ?? '';
    } else if (widget.initialAudioResult != null) {
      _extractedText = widget.initialAudioResult!.transcribedText;
      _descriptionController.text = _extractedText ?? '';
      // Parse commands or text for amount, accounts, etc. (more complex AI task)
    } else if (widget.initialInput != null && widget.initialInput!.inputType == 'text') {
      _extractedText = widget.initialInput!.data;
      _descriptionController.text = _extractedText ?? '';
    }
  }

  String? _findExtractedValue(String fieldName) {
    try {
      return _extractedFields.firstWhere((field) => field.name == fieldName).value;
    } catch (e) {
      return null;
    }
  }

  Future<void> _submitJournalEntry() async {
    if (!_formKey.currentState!.validate()) {
      return;
    }

    setState(() {
      _isProcessing = true;
      _errorMessage = null;
      _successMessage = null;
    });

    try {
      final journalEntry = JournalEntryCreate(
        entryDate: _entryDate,
        description: _descriptionController.text,
        sourceModule: "Multimodal Entry Window",
        referenceNumber: widget.initialInput?.id ?? 'MM-${DateTime.now().millisecondsSinceEpoch}',
        lines: [
          JournalLineCreate(
            accountNumber: _debitAccountController.text,
            debit: Decimal.parse(_amountController.text),
            credit: Decimal.parse('0.00'),
            description: _descriptionController.text,
          ),
          JournalLineCreate(
            accountNumber: _creditAccountController.text,
            debit: Decimal.parse('0.00'),
            credit: Decimal.parse(_amountController.text),
            description: _descriptionController.text,
          ),
        ],
      );

      final AccountingApiService accountingApiService = AccountingApiService();
      await accountingApiService.createJournalEntry(journalEntry, isSynced: false); // Create locally first

      setState(() {
        _successMessage = 'Journal Entry successfully submitted (queued for sync if offline)!';
        _isProcessing = false;
        // Optionally mark the original multimodal task as processed or linked
      });
      // Navigate back or clear form
    } catch (e) {
      setState(() {
        _errorMessage = 'Failed to submit Journal Entry: ${e.toString()}';
        _isProcessing = false;
      });
    }
  }

  Future<void> _provideFeedback(String feedbackType, String comment) async {
    // This would send feedback back to the multimodal service for model improvement
    // E.g., MultimodalApiService.sendFeedback(widget.initialInput?.id, feedbackType, comment, currentExtractedData, userCorrections)
    print('Feedback received: $feedbackType - $comment');
    setState(() {
      _successMessage = 'Feedback sent successfully!';
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Review & Create Entry'),
      ),
      body: Padding(
        padding: const EdgeInsets.all(16.0),
        child: SingleChildScrollView(
          child: Form(
            key: _formKey,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (_extractedText != null) ...[
                  Text('Extracted Text:', style: Theme.of(context).textTheme.headlineSmall),
                  const SizedBox(height: 8),
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(8.0),
                      child: Text(_extractedText!),
                    ),
                  ),
                  const SizedBox(height: 16),
                ],
                Text('Extracted Fields:', style: Theme.of(context).textTheme.headlineSmall),
                const SizedBox(height: 8),
                ..._extractedFields.map((field) => Text('${field.name}: ${field.value}')).toList(),
                const SizedBox(height: 16),
                Text('Journal Entry Details:', style: Theme.of(context).textTheme.headlineSmall),
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
                  controller: _amountController,
                  decoration: const InputDecoration(labelText: 'Amount'),
                  keyboardType: TextInputType.number,
                  validator: (value) {
                    if (value == null || value.isEmpty) {
                      return 'Please enter an amount';
                    }
                    if (Decimal.tryParse(value) == null) {
                      return 'Please enter a valid number';
                    }
                    return null;
                  },
                ),
                TextFormField(
                  controller: _debitAccountController,
                  decoration: const InputDecoration(labelText: 'Debit Account Number'),
                  validator: (value) {
                    if (value == null || value.isEmpty) {
                      return 'Please enter a debit account';
                    }
                    return null;
                  },
                ),
                TextFormField(
                  controller: _creditAccountController,
                  decoration: const InputDecoration(labelText: 'Credit Account Number'),
                  validator: (value) {
                    if (value == null || value.isEmpty) {
                      return 'Please enter a credit account';
                    }
                    return null;
                  },
                ),
                // Date picker for _entryDate
                const SizedBox(height: 16),
                ElevatedButton(
                  onPressed: _isProcessing ? null : _submitJournalEntry,
                  child: _isProcessing
                      ? const CircularProgressIndicator()
                      : const Text('Create Journal Entry'),
                ),
                if (_errorMessage != null)
                  Text(_errorMessage!, style: const TextStyle(color: Colors.red)),
                if (_successMessage != null)
                  Text(_successMessage!, style: const TextStyle(color: Colors.green)),
                const SizedBox(height: 24),
                Text('Feedback for AI Model:', style: Theme.of(context).textTheme.headlineSmall),
                ElevatedButton(
                  onPressed: () => _provideFeedback('incorrect_extraction', 'Extracted data was wrong.'),
                  child: const Text('Data Incorrect'),
                ),
                ElevatedButton(
                  onPressed: () => _provideFeedback('wrong_category', 'Suggested category was wrong.'),
                  child: const Text('Wrong Category'),
                ),
                // More feedback options
              ],
            ),
          ),
        ),
      ),
    );
  }

  @override
  void dispose() {
    _descriptionController.dispose();
    _amountController.dispose();
    _debitAccountController.dispose();
    _creditAccountController.dispose();
    super.dispose();
  }
}
