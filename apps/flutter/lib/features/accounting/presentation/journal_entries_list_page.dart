import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/models/journal_entry_drafting.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entry_detail_page.dart'; // NEW import

class JournalEntriesListPage extends StatefulWidget {
  const JournalEntriesListPage({super.key, this.service});

  /// The engine the page talks to. A test hands it one that answers without a phone.
  final AccountingApiService? service;

  @override
  State<JournalEntriesListPage> createState() => _JournalEntriesListPageState();
}

class _JournalEntriesListPageState extends State<JournalEntriesListPage> {
  late Future<List<JournalEntry>> _journalEntriesFuture;
  late final AccountingApiService _apiService =
      widget.service ?? AccountingApiService();

  @override
  void initState() {
    super.initState();
    _journalEntriesFuture = _apiService.getJournalEntries();
  }

  /// Which draft is being posted, so its row shows work in progress and cannot be posted twice at once.
  String? _posting;

  Future<void> _post(JournalEntry entry) async {
    setState(() => _posting = entry.id);
    final messenger = ScaffoldMessenger.of(context);
    try {
      await _apiService.postJournalEntry(entry.id);
      if (!mounted) return;
      messenger.showSnackBar(
        SnackBar(content: Text('Posted "${entry.description}"')),
      );
      _refreshJournalEntries();
    } catch (error) {
      if (!mounted) return;
      messenger.showSnackBar(
        SnackBar(
          content: Text(
            'Could not post it: ${error.toString().replaceFirst('Exception: ', '')}',
          ),
        ),
      );
    } finally {
      if (mounted) setState(() => _posting = null);
    }
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
                  margin: const EdgeInsets.symmetric(
                    horizontal: 8,
                    vertical: 4,
                  ),
                  child: ListTile(
                    title: Text(entry.description),
                    // A draft sits in this list looking exactly like a posting unless it says so. The statements leave
                    // drafts out, so the row has to admit that or the person believes the books have moved.
                    // A draft can be posted from here, because posting it is the whole point of keeping it: the note
                    // becomes the entry, and the row that said "Draft" says nothing once the books have moved.
                    trailing: isHeldAsDraft(entry)
                        ? Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              const Text(
                                'Draft',
                                style: TextStyle(
                                  color: Colors.orange,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              TextButton(
                                onPressed: _posting == entry.id
                                    ? null
                                    : () => _post(entry),
                                child: _posting == entry.id
                                    ? const SizedBox(
                                        height: 16,
                                        width: 16,
                                        child: CircularProgressIndicator(
                                          strokeWidth: 2,
                                        ),
                                      )
                                    : const Text('Post'),
                              ),
                            ],
                          )
                        : null,
                    subtitle: Text(
                      'Ref: ${entry.referenceNumber ?? 'N/A'} | Date: ${entry.entryDate.toLocal().toString().split(' ')[0]}'
                      '${isHeldAsDraft(entry) ? ' | not in the statements yet' : ''}',
                    ),
                    onTap: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(
                          builder: (context) => JournalEntryDetailPage(
                            entry: entry,
                            service: _apiService,
                          ),
                        ),
                      );
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
