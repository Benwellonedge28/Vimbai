// mobile-client/lib/widgets/multimodal_entry_window.dart

import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:finacc_mobile_client/models/multimodal_models.dart';
import 'package:finacc_mobile_client/models/accounting_models.dart';
import 'package:finacc_mobile_client/services/multimodal_api_service.dart';
import 'package:finacc_mobile_client/services/accounting_api_service.dart';
import 'package:finacc_mobile_client/local_db/local_database.dart';
import 'package:decimal/decimal.dart'; // For Decimal type
import 'package:uuid/uuid.dart'; // For generating unique IDs

class MultimodalEntryWindow extends StatefulWidget {
  final String? taskId; // Optional: if opening an existing task for review

  const MultimodalEntryWindow({Key? key, this.taskId}) : super(key: key);

  @override
  _MultimodalEntryWindowState createState() => _MultimodalEntryWindowState();
}

class _MultimodalEntryWindowState extends State<MultimodalEntryWindow> {
  MultimodalProcessingTaskInDB? _task;
  bool _isLoading = true;
  String? _errorMessage;
  Map<String, TextEditingController> _controllers = {};
  List<ExtractedDataField> _editableFields = [];

  @override
  void initState() {
    super.initState();
    _fetchTask();
  }

  @override
  void dispose() {
    _controllers.values.forEach((controller) => controller.dispose());
    super.dispose();
  }

  Future<void> _fetchTask() async {
    setState(() {
      _isLoading = true;
      _errorMessage = null;
    });
    try {
      if (widget.taskId != null) {
        _task = await Provider.of<MultimodalApiService>(context, listen: false).getTask(widget.taskId!);
      } else {
        // For new inputs, we'd typically create a task first and then fetch it
        // For now, let's mock a simple task if no ID is provided
        _task = MultimodalProcessingTaskInDB(
          id: Uuid().v4(),
          userId: 'mock_user_id', // Replace with actual user ID
          inputType: MultimodalInputType.document,
          createdAt: DateTime.now(),
          updatedAt: DateTime.now(),
          status: MultimodalProcessingStatus.reviewPending,
          documentResult: DocumentParseResult(
            extractedData: [
              ExtractedDataField(name: 'vendor_name', value: 'Mock Mart', confidence: 0.9),
              ExtractedDataField(name: 'date', value: DateTime.now().toIso8601String().split('T')[0], confidence: 0.85),
              ExtractedDataField(name: 'total_amount', value: '123.45', confidence: 0.92),
              ExtractedDataField(name: 'category', value: 'Groceries', confidence: 0.7),
            ],
            aiConfidence: 0.8,
          ),
          suggestedJournalEntry: {
            'description': 'Mock Mart Purchase',
            'amount': '123.45',
            'date': DateTime.now().toIso8601String().split('T')[0],
          }
        );
      }
      _populateEditableFields();
    } catch (e) {
      _errorMessage = 'Failed to load task: $e';
      print(_errorMessage);
    } finally {
      setState(() {
        _isLoading = false;
      });
    }
  }

  void _populateEditableFields() {
    _editableFields.clear();
    _controllers.values.forEach((controller) => controller.dispose()); // Dispose old controllers
    _controllers.clear();

    if (_task?.documentResult != null) {
      _editableFields.addAll(_task!.documentResult!.extractedData);
    } else if (_task?.audioResult != null) {
      _editableFields.addAll(_task!.audioResult!.extractedCommands);
    } else if (_task?.imageResult != null) {
      _editableFields.addAll(_task!.imageResult!.extractedObjects);
    }

    for (var field in _editableFields) {
      _controllers[field.name] = TextEditingController(text: field.value);
    }
  }

  Future<void> _submitCorrections() async {
    if (_task == null) return;

    final multimodalApiService = Provider.of<MultimodalApiService>(context, listen: false);
    final localDatabase = Provider.of<LocalDatabase>(context, listen: false);
    final syncService = Provider.of<SyncService>(context, listen: false);

    try {
      for (var field in _editableFields) {
        final correctedValue = _controllers[field.name]?.text;
        if (correctedValue != null && correctedValue != field.value) {
          final correction = UserCorrection(
            taskId: _task!.id,
            userId: _task!.userId,
            fieldName: field.name,
            originalValue: field.value,
            correctedValue: correctedValue,
            feedbackType: UserCorrectionType.valueCorrection,
            submittedAt: DateTime.now(),
          );
          // Save to local DB first (for offline support)
          await localDatabase.saveUserCorrection(UserCorrectionInDB(
            id: Uuid().v4(), // Generate ID for local storage
            taskId: correction.taskId,
            userId: correction.userId,
            fieldName: correction.fieldName,
            originalValue: correction.originalValue,
            correctedValue: correction.correctedValue,
            feedbackType: correction.feedbackType,
            submittedAt: correction.submittedAt,
          ));
          // Attempt to submit to API. SyncService will handle if offline.
          await multimodalApiService.submitUserCorrection(_task!.id, correction);
          await localDatabase.markUserCorrectionAsSynced(correction.id); // Mark as synced if successful
        }
      }

      // After corrections, update task status (e.g., to 'completed' or 'user_corrected')
      // and potentially create a Journal Entry
      await _createJournalEntryFromSuggested();

      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Corrections submitted and Journal Entry created!')), 
      );
      // Trigger a sync if not already running
      syncService.syncAll();
      Navigator.pop(context); // Go back after submission

    } catch (e) {
      setState(() {
        _errorMessage = 'Failed to submit corrections: $e';
      });
      print(_errorMessage);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Error: $_errorMessage')), 
      );
    }
  }

  Future<void> _createJournalEntryFromSuggested() async {
    if (_task?.suggestedJournalEntry == null) return;

    final accountingApiService = Provider.of<AccountingApiService>(context, listen: false);
    final localDatabase = Provider.of<LocalDatabase>(context, listen: false);

    // Create Journal Lines (mock for now, need actual accounts)
    final entryDate = DateTime.parse(_controllers['date']?.text ?? _task!.suggestedJournalEntry!['date']);
    final amount = Decimal.parse(_controllers['total_amount']?.text ?? _task!.suggestedJournalEntry!['amount']);
    final description = _controllers['description']?.text ?? _task!.suggestedJournalEntry!['description'];

    // Simplified example: Debit an expense, Credit Cash/Bank
    final journalEntryCreate = JournalEntryCreate(
      entryDate: entryDate,
      description: description,
      sourceModule: 'Multimodal', // Indicates origin
      lines: [
        JournalLineCreate(accountNumber: '5000-Groceries', debit: amount, credit: Decimal.zero, description: description),
        JournalLineCreate(accountNumber: '1010-Cash', debit: Decimal.zero, credit: amount, description: description),
      ],
    );

    // Save to local DB first
    final localEntry = JournalEntryInDB(
      id: Uuid().v4(),
      entryDate: journalEntryCreate.entryDate,
      description: journalEntryCreate.description,
      sourceModule: journalEntryCreate.sourceModule,
      referenceNumber: journalEntryCreate.referenceNumber,
      lines: journalEntryCreate.lines.map((e) => JournalLineInDB(
        id: Uuid().v4(),
        accountNumber: e.accountNumber,
        debit: e.debit,
        credit: e.credit,
        description: e.description,
      )).toList(),
      createdAt: DateTime.now(),
      updatedAt: DateTime.now(),
    );
    await localDatabase.saveJournalEntry(localEntry);

    // Then attempt to push to backend
    try {
      final createdEntry = await accountingApiService.createJournalEntry(journalEntryCreate);
      await localDatabase.markJournalEntryAsSynced(localEntry.id); // Mark local as synced if successful
      print('Journal Entry created via API: ${createdEntry.id}');
      
      // Update the multimodal task to link to the new JE
      await Provider.of<MultimodalApiService>(context, listen: false).updateTask(_task!.id, MultimodalProcessingTaskUpdate(
        linkedFinaccEntityId: createdEntry.id,
        status: MultimodalProcessingStatus.completed,
      ));

    } catch (e) {
      print('Failed to create Journal Entry via API: $e. It will be synced later.');
      // The local entry remains unsynced and will be picked up by SyncService
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Multimodal Entry Review'),
        actions: [
          if (!_isLoading) 
            IconButton(
              icon: const Icon(Icons.refresh),
              onPressed: _fetchTask,
            ),
        ],
      ),
      body: _isLoading
          ? const Center(child: CircularProgressIndicator())
          : _errorMessage != null
              ? Center(child: Text('Error: $_errorMessage'))
              : _task == null
                  ? const Center(child: Text('No task data available'))
                  : Padding(
                      padding: const EdgeInsets.all(16.0),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('Review Extracted Data', style: Theme.of(context).textTheme.headlineSmall),
                          const SizedBox(height: 16),
                          Expanded(
                            child: ListView.builder(
                              itemCount: _editableFields.length,
                              itemBuilder: (context, index) {
                                final field = _editableFields[index];
                                return Padding(
                                  padding: const EdgeInsets.symmetric(vertical: 8.0),
                                  child: Row(
                                    children: [
                                      SizedBox(width: 120, child: Text(field.name, style: TextStyle(fontWeight: FontWeight.bold))),
                                      Expanded(
                                        child: TextFormField(
                                          controller: _controllers[field.name],
                                          decoration: InputDecoration(
                                            labelText: 'Corrected ${field.name}',
                                            hintText: field.value,
                                            border: const OutlineInputBorder(),
                                            suffixIcon: field.confidence != null && field.confidence! < 0.8
                                                ? Tooltip(message: 'Low confidence: ${(field.confidence! * 100).toStringAsFixed(0)}%', child: Icon(Icons.warning, color: Colors.orange))
                                                : null,
                                          ),
                                        ),
                                      ),
                                    ],
                                  ),
                                );
                              },
                            ),
                          ),
                          const SizedBox(height: 20),
                          Center(
                            child: ElevatedButton(
                              onPressed: _submitCorrections,
                              child: const Text('Submit Corrections & Create Journal Entry'),
                            ),
                          ),
                        ],
                      ),
                    ),
    );
  }
}
