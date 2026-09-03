// Books page - one app, every audience.
//
// Lists the user's Books (personal, household/family, group, business),
// lets them create new Books, accept invitations, invite members with
// per-membership roles, and run an on-demand sync. The active Book is
// stored locally and scopes the rest of the client.

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:vimbai_mobile_client/models/book_models.dart';
import 'package:vimbai_mobile_client/services/book_sync_service.dart';

class BooksPage extends StatefulWidget {
  const BooksPage({super.key});

  @override
  State<BooksPage> createState() => _BooksPageState();
}

class _BooksPageState extends State<BooksPage> {
  final BookSyncService _sync = BookSyncService.instance;
  List<VBook> _books = [];
  bool _loading = true;
  String? _error;
  String? _activeBookId;

  static const Map<String, IconData> _tierIcons = {
    'personal': Icons.person,
    'household': Icons.home,
    'group': Icons.groups,
    'business': Icons.business_center,
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
      final prefs = await SharedPreferences.getInstance();
      if (!mounted) return;
      setState(() {
        _books = books;
        _activeBookId = prefs.getString('active_book_id');
        _loading = false;
      });
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

  Future<void> _setActive(String bookId) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('active_book_id', bookId);
    if (!mounted) return;
    setState(() => _activeBookId = bookId);
  }

  Future<void> _createBook() async {
    final nameCtrl = TextEditingController();
    String tier = 'household';
    final created = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) {
        return StatefulBuilder(
          builder: (ctx, setDialog) {
            return AlertDialog(
              title: const Text('New Book'),
              content: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextField(
                    controller: nameCtrl,
                    decoration: const InputDecoration(
                      labelText: 'Book name',
                    ),
                  ),
                  const SizedBox(height: 12),
                  ...kBookTiers.map(
                    (t) => RadioListTile<String>(
                      value: t,
                      groupValue: tier,
                      onChanged: (v) => setDialog(() => tier = v ?? tier),
                      title: Text(t[0].toUpperCase() + t.substring(1)),
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
                  onPressed: () => Navigator.pop(
                    ctx,
                    {'name': nameCtrl.text, 'tier': tier},
                  ),
                  child: const Text('Create'),
                ),
              ],
            );
          },
        );
      },
    );
    if (created == null) return;
    final name = (created['name'] ?? '').trim();
    if (name.isEmpty) return;
    try {
      final book = await _sync.createBook(name, created['tier']!);
      await _setActive(book.id);
      await _load();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Could not create book: $e')),
        );
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
              child: ListView.builder(
                itemCount: _books.length,
                itemBuilder: (ctx, i) {
                  final b = _books[i];
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
                          : (isActive ? const Icon(Icons.check_circle) : null),
                      onTap: invited ? null : () => _setActive(b.id),
                      onLongPress: b.yourRole == 'owner' ||
                              b.yourRole == 'admin'
                          ? () => _invite(b.id)
                          : null,
                    ),
                  );
                },
              ),
            ),
    );
  }
}
