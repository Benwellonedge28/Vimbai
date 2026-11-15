// Setting up an organisation, which is what makes a Book shareable.
//
// A person who registers on a phone gets an account and can open a Book, but a Book that belongs to nobody but them cannot be
// handed to an accountant, a partner or a bookkeeper. The organisation is that middle step, and until now the app had no way to
// take it - the engine has answered the routes all along.
//
// The page is deliberately plain about its three states: still asking, nothing set up yet, and set up. It never pretends an
// organisation exists when the call failed, because a local row that disagrees with the engine is worse than an honest error.

import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/features/authentication/data/auth_service.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';
import 'package:vimbai_mobile_client/features/books/models/book_models.dart';
import 'package:vimbai_mobile_client/features/organisations/data/organisation_service.dart';
import 'package:vimbai_mobile_client/features/organisations/models/organisation_models.dart';

class OrganisationPage extends StatefulWidget {
  const OrganisationPage({super.key, this.service, this.localBooks});

  /// Handed in by a test so the page can be walked without a network; the app never passes one.
  final OrganisationService? service;

  /// The Books on this device. Handed in by a test for the same reason as [service]; the app reads the local store.
  final Future<List<VBook>> Function()? localBooks;

  @override
  State<OrganisationPage> createState() => _OrganisationPageState();
}

class _OrganisationPageState extends State<OrganisationPage> {
  late final OrganisationService _service =
      widget.service ?? OrganisationService();
  late final Future<List<VBook>> Function() _localBooks =
      widget.localBooks ?? BookSyncService.instance.localBooks;
  final TextEditingController _name = TextEditingController();

  bool _loading = true;
  bool _busy = false;
  String? _error;
  List<VOrganisation> _mine = const [];

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final token = await AuthService().getToken();
      _service.useToken(token);
      final mine = await _service.mine();
      if (!mounted) return;
      setState(() {
        _mine = mine;
        _loading = false;
      });
    } on OrganisationException catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.message;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      // Offline is the common case on a phone and deserves the plain words rather than a stack trace.
      setState(() {
        _error = 'Could not reach the server. $e';
        _loading = false;
      });
    }
  }

  Future<void> _create() async {
    final name = _name.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Give the organisation a name.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final made = await _service.create(name);
      if (!mounted) return;
      _name.clear();
      setState(() {
        _mine = [..._mine, made];
        _busy = false;
      });
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('${made.name} is open. You are its owner.')),
      );
    } on OrganisationException catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = e.message;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = 'Could not reach the server. $e';
      });
    }
  }

  /// Offer the Books on this device that are not already inside [organisation], and put the chosen one in.
  ///
  /// Both lists are asked for at the moment of the tap rather than kept from the load: a Book may have been made since, and an
  /// organisation's Books change from other devices too.
  Future<void> _putBook(VOrganisation organisation) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final inside = (await _service.books(organisation.id)).toSet();
      final mine = await _localBooks();
      final candidates = mine.where((b) => !inside.contains(b.id)).toList();
      if (!mounted) return;
      setState(() => _busy = false);
      if (candidates.isEmpty) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              inside.isEmpty
                  ? 'There are no Books on this device to put in.'
                  : 'Every Book on this device is already in '
                        '${organisation.name}.',
            ),
          ),
        );
        return;
      }
      final chosen = await showDialog<VBook>(
        context: context,
        builder: (ctx) => SimpleDialog(
          title: Text('Put which Book in ${organisation.name}?'),
          children: candidates
              .map(
                (book) => SimpleDialogOption(
                  onPressed: () => Navigator.of(ctx).pop(book),
                  child: Text(book.name),
                ),
              )
              .toList(),
        ),
      );
      if (chosen == null || !mounted) return;
      await _moveIn(organisation, chosen);
    } on OrganisationException catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = e.message;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = 'Could not reach the server. $e';
      });
    }
  }

  Future<void> _moveIn(VOrganisation organisation, VBook book) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await _service.moveBook(organisation.id, book.id);
      if (!mounted) return;
      setState(() => _busy = false);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('${book.name} is now in ${organisation.name}.')),
      );
    } on OrganisationException catch (e) {
      if (!mounted) return;
      // A refusal here is a real answer - the engine wants both halves of the permission - so it is shown, not retried.
      setState(() {
        _busy = false;
        _error = e.message;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = 'Could not reach the server. $e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Your organisations')),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.all(16),
              children: [
                const Text(
                  'An organisation owns Books. Put a Book in one and you can '
                  'share it with the people who help you keep it - an '
                  'accountant, a partner, a bookkeeper - without handing over '
                  'your password.',
                ),
                const SizedBox(height: 16),
                if (_error != null) ...[
                  Text(
                    _error!,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                  const SizedBox(height: 12),
                ],
                // Only said when the engine actually answered. A failed call leaves _mine empty too, and telling somebody
                // they own nothing because the server did not reply is the one lie this page must not tell: they would go
                // and open a second organisation they already have.
                if (_mine.isEmpty && _error == null)
                  const Text(
                    'You have not set one up yet.',
                    style: TextStyle(fontStyle: FontStyle.italic),
                  )
                else
                  ..._mine.map(
                    (organisation) => Card(
                      child: ListTile(
                        leading: const Icon(Icons.apartment),
                        title: Text(organisation.name),
                        subtitle: Text('You are its ${organisation.role}'),
                      ),
                    ),
                  ),
                const SizedBox(height: 20),
                TextField(
                  controller: _name,
                  textInputAction: TextInputAction.done,
                  onSubmitted: (_) => _busy ? null : _create(),
                  decoration: const InputDecoration(
                    labelText: 'Name',
                    helperText:
                        'The name the people sharing your Books will see.',
                  ),
                ),
                const SizedBox(height: 12),
                _busy
                    ? const Center(child: CircularProgressIndicator())
                    : ElevatedButton(
                        onPressed: _create,
                        child: const Text('Open an organisation'),
                      ),
                if (_mine.isNotEmpty) ...[
                  const SizedBox(height: 24),
                  const Text(
                    'A Book only becomes shareable once it is inside an '
                    'organisation. Put one here and the people you invite to '
                    'the organisation can work on it.',
                  ),
                  const SizedBox(height: 8),
                  ..._mine
                      .where((o) => o.canManageBooks)
                      .map(
                        (organisation) => OutlinedButton.icon(
                          icon: const Icon(Icons.library_add),
                          label: Text('Put a Book in ${organisation.name}'),
                          onPressed: _busy
                              ? null
                              : () => _putBook(organisation),
                        ),
                      ),
                ],
              ],
            ),
    );
  }
}
