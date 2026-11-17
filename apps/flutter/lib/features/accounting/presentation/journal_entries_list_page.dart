import 'package:flutter/material.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/models/accounting_models.dart';
import 'package:vimbai_mobile_client/features/accounting/models/journal_entry_drafting.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entry_detail_page.dart'; // NEW import
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entry_form_page.dart';

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

  /// Which draft is being thrown away, for the same reason: one row, one piece of work.
  String? _discarding;

  /// Throws a note away after asking. A note never became an entry, so nothing in the books moves; a row that stays is a
  /// wrong figure waiting to be posted by mistake.
  Future<void> _discard(JournalEntry entry) async {
    final agreed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Throw this note away?'),
        content: Text(
          '"${entry.description}" was saved as a note and never posted, so nothing in the statements moves. Throwing it '
          'away cannot be undone.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialog).pop(false),
            child: const Text('Keep it'),
          ),
          FilledButton.tonal(
            onPressed: () => Navigator.of(dialog).pop(true),
            child: const Text('Throw it away'),
          ),
        ],
      ),
    );
    if (agreed != true || !mounted) return;

    setState(() => _discarding = entry.id);
    final messenger = ScaffoldMessenger.of(context);
    try {
      await _apiService.deleteJournalEntry(entry.id);
      if (!mounted) return;
      messenger.showSnackBar(
        SnackBar(content: Text('Threw away "${entry.description}"')),
      );
      _refreshJournalEntries();
    } catch (error) {
      if (!mounted) return;
      messenger.showSnackBar(
        SnackBar(
          content: Text(
            'Could not throw it away: ${error.toString().replaceFirst('Exception: ', '')}',
          ),
        ),
      );
    } finally {
      if (mounted) setState(() => _discarding = null);
    }
  }

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

  /// A read that failed is reported as a failed read, in the engine's own words, and never as an empty Book: a person
  /// who has posted entries and is shown none would think their work was lost.
  Widget _couldNotRead(Object? error) {
    final reason = (error ?? 'no reason given').toString().replaceFirst(
      'Exception: ',
      '',
    );
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.cloud_off, size: 48),
            const SizedBox(height: 12),
            const Text(
              'The entries could not be read just now.',
              style: TextStyle(fontWeight: FontWeight.bold),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 6),
            Text(reason, textAlign: TextAlign.center),
            const SizedBox(height: 16),
            OutlinedButton(
              onPressed: _refreshJournalEntries,
              child: const Text('Try again'),
            ),
          ],
        ),
      ),
    );
  }

  /// The first-run moment: a Book that has just been made has a chart of accounts and nothing in it. An empty list on
  /// its own reads as a dead end, so the page says what an entry is and offers the one step that follows.
  Widget _nothingPostedYet() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.receipt_long_outlined, size: 56),
            const SizedBox(height: 12),
            const Text(
              'Nothing posted in this Book yet',
              style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            const Text(
              'An entry is what moves the numbers: what came in or went out, and which '
              'accounts it touches. This Book already has its chart of accounts, so you '
              'can pick them by name instead of remembering numbers.',
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 20),
            FilledButton.icon(
              onPressed: _writeTheFirstEntry,
              icon: const Icon(Icons.add),
              label: const Text('Write the first entry'),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _writeTheFirstEntry() async {
    await Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => JournalEntryFormPage(service: _apiService),
      ),
    );
    // Whatever the form did, the list is asked again rather than assumed: a note saved there is a row here.
    if (mounted) _refreshJournalEntries();
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
            return _couldNotRead(snapshot.error);
          } else if (!snapshot.hasData) {
            // A read that came back empty-handed is not a Book with no entries, and saying so would be a lie about
            // somebody's books.
            return const Center(
              child: Text('The entries could not be read just now.'),
            );
          } else if (snapshot.data!.isEmpty) {
            return _nothingPostedYet();
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
                              TextButton(
                                onPressed: _discarding == entry.id
                                    ? null
                                    : () => _discard(entry),
                                child: _discarding == entry.id
                                    ? const SizedBox(
                                        height: 16,
                                        width: 16,
                                        child: CircularProgressIndicator(
                                          strokeWidth: 2,
                                        ),
                                      )
                                    : const Text('Discard'),
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
