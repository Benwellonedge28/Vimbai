// Find one thing in this Book.
//
// A Book kept for a year holds thousands of entries, and the only way to reach one was to remember which screen it was on: a
// receipt was in the cash book, an invoice in the invoices list, a supplier's bill in what you owe, a correction in the
// journal. This is the one place to type what you remember.
//
// The engine searches and answers; the screen draws the answer, grouped into what the entries were and what the accounts
// were, and says for each hit which part of it matched so a person can see why it is in the list. It never matches or ranks
// anything itself, and a read that failed says so rather than looking like "nothing found".
import 'dart:async';

import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/accounting/data/accounting_api_service.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entry_detail_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/ledger_page.dart';
import 'package:vimbai_mobile_client/features/find/data/find_service.dart';

class FindPage extends StatefulWidget {
  const FindPage({super.key, this.service});

  /// A test answers the search itself, so the page can be exercised without a server.
  final FindService? service;

  @override
  State<FindPage> createState() => _FindPageState();
}

class _FindPageState extends State<FindPage> {
  late final FindService _service = widget.service ?? FindService.instance;

  final TextEditingController _words = TextEditingController();

  /// The search waits until the typing stops, so a letter-by-letter search does not read the Book five times.
  Timer? _pause;
  FindAnswer? _answer;
  bool _reading = false;

  /// Which search is the current one. An answer to an older one is dropped rather than drawn, so what is on screen always
  /// belongs to the words in the box.
  int _asked = 0;
  String? _trouble;

  @override
  void dispose() {
    _pause?.cancel();
    _words.dispose();
    super.dispose();
  }

  void _typed(String value) {
    _pause?.cancel();
    final words = value.trim();
    if (words.length < 2) {
      setState(() {
        _answer = null;
        _trouble = null;
        _reading = false;
      });
      return;
    }
    _pause = Timer(const Duration(milliseconds: 350), () => _search(words));
  }

  Future<void> _search(String words) async {
    final mine = ++_asked;
    setState(() {
      _reading = true;
      _trouble = null;
    });
    try {
      final answer = await _service.find(words);
      if (!mounted || mine != _asked) return;
      setState(() {
        _answer = answer;
        _reading = false;
      });
    } catch (failure) {
      if (!mounted) return;
      setState(() {
        _reading = false;
        _trouble = CouldNotRead.reasonOf(failure);
      });
    }
  }

  void _open(FoundThing found) {
    if (found.kind == 'account') {
      Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (context) => LedgerPage(account: found.id),
        ),
      );
      return;
    }
    // an entry is read in full before it is shown: the search answers where it is, not what it says, and a detail page
    // drawn from a search hit would be a page with its lines missing
    _openEntry(found.id);
  }

  Future<void> _openEntry(String id) async {
    setState(() => _trouble = null);
    try {
      final entry = await AccountingApiService().getJournalEntry(id);
      if (!mounted) return;
      Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (context) => JournalEntryDetailPage(entry: entry),
        ),
      );
    } catch (failure) {
      if (!mounted) return;
      setState(() => _trouble = CouldNotRead.reasonOf(failure));
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final answer = _answer;
    return Scaffold(
      appBar: AppBar(title: const Text('Find in this Book')),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 32),
        children: [
          TextField(
            controller: _words,
            autofocus: true,
            textInputAction: TextInputAction.search,
            decoration: const InputDecoration(
              labelText: 'What do you remember',
              helperText:
                  'A reference, a description, or the name or number of an account.',
              border: OutlineInputBorder(),
              prefixIcon: Icon(Icons.search),
            ),
            onChanged: _typed,
          ),
          const SizedBox(height: 16),
          if (_reading) const LinearProgressIndicator(),
          if (_trouble != null)
            Text(
              _trouble!,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          if (answer != null && answer.foundNothing)
            Text(
              'Nothing in this Book holds "${answer.query}".',
              style: theme.textTheme.bodyMedium,
            ),
          if (answer != null && answer.entries.isNotEmpty) ...[
            Text('Entries', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            for (final found in answer.entries)
              ListTile(
                contentPadding: EdgeInsets.zero,
                dense: true,
                title: Text(found.title),
                subtitle: Text(
                  found.isNotPosted
                      ? '${found.subtitle} · not in the statements yet'
                      : found.subtitle,
                ),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => _open(found),
              ),
          ],
          if (answer != null && answer.accounts.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text('Accounts', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            for (final found in answer.accounts)
              ListTile(
                contentPadding: EdgeInsets.zero,
                dense: true,
                title: Text(found.title),
                subtitle: Text(found.subtitle),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => _open(found),
              ),
          ],
        ],
      ),
    );
  }
}
