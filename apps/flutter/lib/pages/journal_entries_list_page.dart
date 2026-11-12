import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/models/accounting_models.dart';
import 'package:vimbai_mobile_client/pages/journal_entry_detail_page.dart'; // NEW import

class JournalEntriesListPage extends StatefulWidget {
  const JournalEntriesListPage({super.key});

  @override
  State<JournalEntriesListPage> createState() => _JournalEntriesListPageState();
}

class _JournalEntriesListPageState extends State<JournalEntriesListPage> {
  late Future<List<JournalEntry>> _journalEntriesFuture;
  final AccountingApiService _apiService = AccountingApiService();

  @override
  void initState() {
    super.initState();
    _journalEntriesFuture = _apiService.getJournalEntries();
  }

  void _refreshJournalEntries() {
    setState(() {
      _journalEntriesFuture = _apiService.getJournalEntries();
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
          appBar: AppBar(
            title: const Text('Journal Entries'),
            actions: [
              IconButton(
                icon: const Icon(Icons.refresh),
                onPressed: _refreshJournalEntries,
              ),
            ],
          ),
          body: FutureBuilder<List<JournalEntry>>(
            future: _journalEntriesFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              } else if (snapshot.hasError) {
                return Center(child: Text('Error: ${snapshot.error}'));
              } else if (!snapshot.hasData || snapshot.data!.isEmpty) {
                return const Center(child: Text('No journal entries found.'));
              } else {
                return ListView.builder(
                  itemCount: snapshot.data!.length,
                  itemBuilder: (context, index) {
                    final entry = snapshot.data![index];
                    return Card(
                      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                      child: ListTile(
                        title: Text(entry.description),
                        subtitle: Text('Ref: ${entry.referenceNumber ?? 'N/A'} | Date: ${entry.entryDate.toLocal().toString().split(' ')[0]}'),
                        onTap: () {
                          Navigator.of(context).push(MaterialPageRoute(
                            builder: (context) => JournalEntryDetailPage(entry: entry),
                          ));
                        },
                      ),
                    );
                  },
                );
              }
            },
          ),
        );
      }
    }
