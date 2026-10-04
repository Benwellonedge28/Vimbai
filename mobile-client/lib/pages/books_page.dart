// Books page - one app, every audience.
//
// Lists the user's Books (personal, household/family, group, business),
// lets them create new Books, accept invitations, invite members with
// per-membership roles, and run an on-demand sync. The active Book is
// stored locally and scopes the rest of the client.

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:vimbai_mobile_client/models/book_models.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';
import 'package:vimbai_mobile_client/services/book_sync_service.dart';
import 'package:vimbai_mobile_client/pages/create_book_wizard.dart';
import 'package:vimbai_mobile_client/services/npo_scale_service.dart';

class BooksPage extends StatefulWidget {
  const BooksPage({super.key});

  @override
  State<BooksPage> createState() => _BooksPageState();
}

class _BooksPageState extends State<BooksPage> {
  final BookSyncService _sync = BookSyncService.instance;
  final NpoScaleService _scale = NpoScaleService.instance;
  List<VBook> _books = [];
  List<BookContextBook> _orgBooks = [];
  bool _loading = true;
  String? _error;
  String? _activeBookId;

  static const Map<String, IconData> _tierIcons = {
    'personal': Icons.person,
    'household': Icons.home,
    'group': Icons.groups,
    'business': Icons.business_center,
    'nonprofit': Icons.volunteer_activism,
  };

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final books = await _sync.refreshBooksFromServer();
      final orgBooks = await _loadOrgBooks();
      final prefs = await SharedPreferences.getInstance();
      if (!mounted) return;
      setState(() {
        _books = books;
        _orgBooks = orgBooks;
        _activeBookId = prefs.getString('active_book_id');
        _loading = false;
      });
      _hydrateContext();
    } catch (e) {
      final local = await _sync.localBooks();
      if (!mounted) return;
      setState(() {
        _books = local;
        _loading = false;
        _error = 'Offline - showing local books only';
      });
    }
  }

  /// Organization Books: real Books wired to each of the user's orgs
  /// (company, partnership, sole trader, NPO - any scale band). Fetched
  /// independently so a scale-service outage never hides personal Books.
  Future<List<BookContextBook>> _loadOrgBooks() async {
    try {
      final orgs = await _scale.myOrgs();
      final out = <BookContextBook>[];
      for (final org in orgs) {
        final b = await _scale.orgBook(org.id);
        if (b != null) {
          b['name'] = (b['name'] as String?) ?? org.name;
          out.add(BookContextBook.fromJson(b, source: 'org'));
        }
      }
      return out;
    } catch (_) {
      return [];
    }
  }

  /// Keep the app-wide BookContext in sync with the active selection so
  /// every service client sends X-Book-ID for the right context.
  void _hydrateContext() {
    final id = _activeBookId;
    if (id == null) {
      BookContext.instance.clear();
      return;
    }
    for (final b in _books) {
      if (b.id == id && b.membershipStatus == 'active') {
        BookContext.instance.setBook(BookContextBook(
          id: b.id,
          name: b.name,
          tier: b.tier,
          yourRole: b.yourRole,
          source: 'sync',
        ));
        return;
      }
    }
    for (final b in _orgBooks) {
      if (b.id == id) {
        BookContext.instance.setBook(b);
        return;
      }
    }
    BookContext.instance.clear();
  }

  Future<void> _setActive(String bookId) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('active_book_id', bookId);
    if (!mounted) return;
    setState(() => _activeBookId = bookId);
    _hydrateContext();
  }

  Future<void> _createBook() async {
    // Start a Book: choose category -> name it -> create (full wizard).
    final created = await Navigator.of(context).push<String>(
      MaterialPageRoute(builder: (context) => const CreateBookWizardPage()),
    );
    if (created is String && created.isNotEmpty) {
      // The freshly created Book becomes the active Book context right away.
      await _setActive(created);
    }
    await _load();
  }

  /// Books organized into folders: returns folder names sorted, with
  /// books sorted by name inside each folder.
  Map<String, List<VBook>> _folders(List<VBook> books) {
    final byFolder = <String, List<VBook>>{};
    for (final b in books) {
      byFolder.putIfAbsent(b.folder, () => []).add(b);
    }
    final keys = byFolder.keys.where((f) => f.isNotEmpty).toList()..sort();
    if (byFolder.containsKey('')) {
      keys.add(''); // unfiled Books come last
    }
    return {for (final k in keys) k: byFolder[k]!..sort((a, b) => a.name.compareTo(b.name))};
  }

  /// Move a Book into a folder ("" unfiles it).
  Future<void> _moveToFolder(VBook book) async {
    final folders = _books
        .map((b) => b.folder)
        .where((f) => f.isNotEmpty)
        .toSet()
        .toList()
      ..sort();
    final ctrl = TextEditingController(text: book.folder);
    final folder = await showDialog<String>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialog) {
          return AlertDialog(
            title: Text('Move "${book.name}"'),
            content: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (folders.isNotEmpty)
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      ...folders.map(
                        (f) => ChoiceChip(
                          label: Text(f),
                          selected: ctrl.text == f,
                          onSelected: (_) => setDialog(() => ctrl.text = f),
                        ),
                      ),
                      ActionChip(
                        avatar: const Icon(
                          Icons.create_new_folder_outlined,
                          size: 18,
                        ),
                        label: const Text('New'),
                        onPressed: () async {
                          final c2 = TextEditingController();
                          final name = await showDialog<String>(
                            context: ctx,
                            builder: (ctx2) => AlertDialog(
                              title: const Text('New folder'),
                              content: TextField(
                                controller: c2,
                                autofocus: true,
                                maxLength: 120,
                                decoration: const InputDecoration(
                                  labelText: 'Folder name',
                                ),
                              ),
                              actions: [
                                TextButton(
                                  onPressed: () => Navigator.pop(ctx2),
                                  child: const Text('Cancel'),
                                ),
                                ElevatedButton(
                                  onPressed: () =>
                                      Navigator.pop(ctx2, c2.text.trim()),
                                  child: const Text('Add'),
                                ),
                              ],
                            ),
                          );
                          if (name != null && name.isNotEmpty) {
                            setDialog(() => ctrl.text = name);
                          }
                        },
                      ),
                    ],
                  ),
                const SizedBox(height: 12),
                TextField(
                  controller: ctrl,
                  maxLength: 120,
                  decoration: const InputDecoration(
                    labelText: 'Folder name (empty = no folder)',
                  ),
                ),
              ],
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(ctx),
                child: const Text('Cancel'),
              ),
              ElevatedButton(
                onPressed: () => Navigator.pop(ctx, ctrl.text.trim()),
                child: const Text('Move'),
              ),
            ],
          );
        },
      ),
    );
    if (folder == null) return;
    try {
      await _sync.updateBook(book.id, folder: folder);
      await _load();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              folder.isEmpty ? 'Book unfiled' : 'Moved to "$folder"',
            ),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Move failed: $e')),
        );
      }
    }
  }

  /// Long-press actions on a Book: move to a folder, invite members.
  Future<void> _bookActions(VBook book, String folder) async {
    final canInvite = book.yourRole == 'owner' || book.yourRole == 'admin';
    final action = await showModalBottomSheet<String>(
      context: context,
      builder: (ctx) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.all(16),
              child: Text(
                book.name,
                style: Theme.of(context).textTheme.titleMedium,
              ),
            ),
            ListTile(
              leading: const Icon(Icons.drive_file_move_outline),
              title: Text(folder.isEmpty ? 'Move to folder' : 'Change folder'),
              onTap: () => Navigator.pop(ctx, 'move'),
            ),
            if (canInvite)
              ListTile(
                leading: const Icon(Icons.person_add_alt),
                title: const Text('Invite member'),
                onTap: () => Navigator.pop(ctx, 'invite'),
              ),
            if (folder.isNotEmpty)
              ListTile(
                leading: const Icon(Icons.folder_off_outlined),
                title: const Text('Remove from folder'),
                onTap: () => Navigator.pop(ctx, 'unfile'),
              ),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
    switch (action) {
      case 'move':
        await _moveToFolder(book);
      case 'invite':
        await _invite(book.id);
      case 'unfile':
        try {
          await _sync.updateBook(book.id, folder: '');
          await _load();
        } catch (e) {
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text('Could not unfile: $e')),
            );
          }
        }
    }
  }

  Future<void> _invite(String bookId) async {
    final userIdCtrl = TextEditingController();
    String role = 'bookkeeper';
    final res = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) {
        return StatefulBuilder(
          builder: (ctx, setDialog) {
            return AlertDialog(
              title: const Text('Invite member'),
              content: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextField(
                    controller: userIdCtrl,
                    decoration: const InputDecoration(
                      labelText: 'Member user ID',
                    ),
                  ),
                  DropdownButton<String>(
                    value: role,
                    items: const [
                      DropdownMenuItem(value: 'admin', child: Text('Admin')),
                      DropdownMenuItem(
                        value: 'treasurer',
                        child: Text('Treasurer'),
                      ),
                      DropdownMenuItem(
                        value: 'bookkeeper',
                        child: Text('Bookkeeper'),
                      ),
                      DropdownMenuItem(value: 'viewer', child: Text('Viewer')),
                    ],
                    onChanged: (v) => setDialog(() => role = v ?? role),
                  ),
                ],
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(ctx),
                  child: const Text('Cancel'),
                ),
                ElevatedButton(
                  onPressed: () => Navigator.pop(
                    ctx,
                    {'user_id': userIdCtrl.text.trim(), 'role': role},
                  ),
                  child: const Text('Invite'),
                ),
              ],
            );
          },
        );
      },
    );
    if (res == null || (res['user_id'] ?? '').isEmpty) return;
    try {
      await _sync.inviteMember(bookId, res['user_id']!, res['role']!);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Invitation sent')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Invite failed: $e')),
        );
      }
    }
  }

  Future<void> _accept(VBook book) async {
    try {
      await _sync.acceptInvite(book.id);
      await _load();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Accept failed: $e')),
        );
      }
    }
  }

  Future<void> _syncNow() async {
    try {
      final results = await _sync.syncAllSharedBooks();
      var pushed = 0;
      var pulled = 0;
      for (final r in results.values) {
        pushed += r['pushed'] ?? 0;
        pulled += r['pulled'] ?? 0;
      }
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Synced: pushed $pushed, pulled $pulled')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Sync failed: $e')),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Your Books'),
        actions: [
          IconButton(
            icon: const Icon(Icons.sync),
            tooltip: 'Sync now',
            onPressed: _syncNow,
          ),
          IconButton(
            icon: const Icon(Icons.add),
            tooltip: 'New book',
            onPressed: _createBook,
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : RefreshIndicator(
              onRefresh: _load,
              child: ListView(
                children: [
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.all(12),
                      child: Text(
                        _error!,
                        style: TextStyle(color: Theme.of(context).colorScheme.error),
                      ),
                    ),
                  ..._folders(_books).entries.expand((entry) {
                    final folder = entry.key;
                    final books = entry.value;
                    return [
                      if (folder.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                          child: Row(
                            children: [
                              const Icon(Icons.folder_outlined, size: 18),
                              const SizedBox(width: 6),
                              Text(
                                folder,
                                style: Theme.of(context).textTheme.titleSmall,
                              ),
                            ],
                          ),
                        ),
                      ...books.map(
                        (b) {
                          final isActive = b.id == _activeBookId;
                          final invited = b.membershipStatus == 'invited';
                          return Card(
                            color: isActive
                                ? Theme.of(context).colorScheme.primaryContainer
                                : null,
                            child: ListTile(
                              leading: Icon(_tierIcons[b.tier] ?? Icons.book),
                              title: Text(b.name),
                              subtitle: Text(
                                '${b.tier} - you are ${b.yourRole}'
                                '${invited ? ' (invited)' : ''}',
                              ),
                              trailing: invited
                                  ? TextButton(
                                      onPressed: () => _accept(b),
                                      child: const Text('Accept'),
                                    )
                                  : (isActive
                                      ? const Icon(Icons.check_circle)
                                      : null),
                              onTap: invited ? null : () => _setActive(b.id),
                              onLongPress: () =>
                                  _bookActions(b, entry.key),
                            ),
                          );
                        },
                      ),
                    ];
                  }),
                  if (_orgBooks.isNotEmpty) ...[
                    Padding(
                      padding: const EdgeInsets.fromLTRB(16, 20, 16, 4),
                      child: Text(
                        'ORGANIZATION BOOKS',
                        style: Theme.of(context).textTheme.labelSmall,
                      ),
                    ),
                    ..._orgBooks.map(
                      (b) {
                        final isActive = b.id == _activeBookId;
                        return Card(
                          color: isActive
                              ? Theme.of(context).colorScheme.primaryContainer
                              : null,
                          child: ListTile(
                            leading: const Icon(Icons.corporate_fare),
                            title: Text(b.name),
                            subtitle: Text(
                              'org book - ${b.tier} - you are ${b.yourRole}',
                            ),
                            trailing: isActive
                                ? const Icon(Icons.check_circle)
                                : null,
                            onTap: () => _setActive(b.id),
                          ),
                        );
                      },
                    ),
                  ],
                  const SizedBox(height: 24),
                ],
              ),
            ),
    );
  }
}
