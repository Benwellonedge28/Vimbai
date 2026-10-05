// Per-Book settings sheet.
//
// Opened from the kebab (⋮) menu on each Book card. Every setting here
// applies to THIS Book only: its name, description, folder, members and
// sync. Two pages (home list + Books page) share this sheet.

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:vimbai_mobile_client/models/book_models.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';
import 'package:vimbai_mobile_client/services/book_sync_service.dart';

const Map<String, IconData> _tierIcons = {
  'personal': Icons.person,
  'household': Icons.home,
  'group': Icons.groups,
  'business': Icons.business_center,
  'nonprofit': Icons.volunteer_activism,
};

/// Opens the settings sheet for [book]. [folders] supplies existing
/// folder names for the move-to-folder picker. [onChanged] fires after
/// any mutation so the parent list can refresh.
Future<void> showBookSettingsSheet(
  BuildContext context,
  VBook book,
  List<String> folders, {
  required VoidCallback onChanged,
}) {
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (ctx) => _BookSettingsSheet(book: book, folders: folders, onChanged: onChanged),
  );
}

class _BookSettingsSheet extends StatefulWidget {
  final VBook book;
  final List<String> folders;
  final VoidCallback onChanged;

  const _BookSettingsSheet({
    required this.book,
    required this.folders,
    required this.onChanged,
  });

  @override
  State<_BookSettingsSheet> createState() => _BookSettingsSheetState();
}

class _BookSettingsSheetState extends State<_BookSettingsSheet> {
  final BookSyncService _sync = BookSyncService.instance;
  late VBook _book; // mutates as settings change
  bool _isActive = false;
  bool get _canManage => _book.yourRole == 'owner' || _book.yourRole == 'admin';
  List<BookMember> _members = [];
  bool _membersLoading = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _book = widget.book;
    _loadActive();
    if (_canManage) _loadMembers();
  }

  Future<void> _loadActive() async {
    final prefs = await SharedPreferences.getInstance();
    if (mounted) setState(() => _isActive = prefs.getString('active_book_id') == _book.id);
  }

  Future<void> _loadMembers() async {
    setState(() => _membersLoading = true);
    try {
      final ms = await _sync.members(_book.id);
      if (mounted) setState(() { _members = ms; _membersLoading = false; _error = null; });
    } catch (e) {
      if (mounted) setState(() { _membersLoading = false; _error = 'Members unavailable offline'; });
    }
  }

  void _toast(String msg) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));

  // -- settings actions -----------------------------------------------

  Future<void> _rename() async {
    final ctrl = TextEditingController(text: _book.name);
    final name = await _dialog(
      title: 'Rename Book',
      fieldLabel: 'Book name',
      ctrl: ctrl,
      action: 'Rename',
    );
    if (name == null || name.trim().isEmpty || name.trim() == _book.name) return;
    await _mutate(() => _sync.updateBook(_book.id, name: name.trim()));
    if (!mounted) return;
    setState(() => _book = _book.copyWith(name: name.trim()));
  }

  Future<void> _editDescription() async {
    final ctrl = TextEditingController(text: _book.description);
    final desc = await _dialog(
      title: 'Description',
      fieldLabel: 'What is this Book for?',
      ctrl: ctrl,
      action: 'Save',
    );
    if (desc == null) return;
    await _mutate(() => _sync.updateBook(_book.id, description: desc.trim()));
    if (!mounted) return;
    setState(() => _book = _book.copyWith(description: desc.trim()));
  }

  Future<void> _moveToFolder() async {
    final existing = widget.folders.toSet().toList()..sort();
    final ctrl = TextEditingController(text: _book.folder);
    final folder = await showDialog<String>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialog) => AlertDialog(
          title: Text(_book.folder.isEmpty ? 'Move to folder' : 'Change folder'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (existing.isNotEmpty)
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    ...existing.map(
                      (f) => ChoiceChip(
                        label: Text(f),
                        selected: ctrl.text == f,
                        onSelected: (_) => setDialog(() => ctrl.text = f),
                      ),
                    ),
                  ],
                ),
              if (existing.isNotEmpty) const SizedBox(height: 12),
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
            TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
            ElevatedButton(
              onPressed: () => Navigator.pop(ctx, ctrl.text.trim()),
              child: const Text('Save'),
            ),
          ],
        ),
      ),
    );
    if (folder == null || folder == _book.folder) return;
    await _mutate(() => _sync.updateBook(_book.id, folder: folder));
    if (!mounted) return;
    setState(() => _book = _book.copyWith(folder: folder));
    _toast(folder.isEmpty ? 'Book unfiled' : 'Moved to "$folder"');
  }

  Future<void> _makeActive() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('active_book_id', _book.id);
    if (_book.membershipStatus == 'active') {
      BookContext.instance.setBook(BookContextBook(
        id: _book.id,
        name: _book.name,
        tier: _book.tier,
        yourRole: _book.yourRole,
        source: 'sync',
      ));
    }
    if (!mounted) return;
    setState(() => _isActive = true);
    widget.onChanged();
    _toast('"${_book.name}" is now your active Book');
  }

  Future<void> _syncNow() async {
    try {
      final r = await _sync.syncBook(_book.id);
      if (!mounted) return;
      _toast('Synced "${_book.name}": pushed ${r['pushed'] ?? 0}, pulled ${r['pulled'] ?? 0}');
    } catch (e) {
      if (!mounted) return;
      _toast('Sync failed: $e');
    }
  }

  Future<void> _invite() async {
    final userIdCtrl = TextEditingController();
    String role = 'bookkeeper';
    final res = await showDialog<Map<String, String>>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setDialog) => AlertDialog(
          title: Text('Invite to "${_book.name}"'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                controller: userIdCtrl,
                decoration: const InputDecoration(labelText: 'Member user ID'),
              ),
              DropdownButton<String>(
                value: role,
                isExpanded: true,
                items: const [
                  DropdownMenuItem(value: 'admin', child: Text('Admin')),
                  DropdownMenuItem(value: 'treasurer', child: Text('Treasurer')),
                  DropdownMenuItem(value: 'bookkeeper', child: Text('Bookkeeper')),
                  DropdownMenuItem(value: 'viewer', child: Text('Viewer')),
                ],
                onChanged: (v) => setDialog(() => role = v ?? role),
              ),
            ],
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
            ElevatedButton(
              onPressed: () =>
                  Navigator.pop(ctx, {'user_id': userIdCtrl.text.trim(), 'role': role}),
              child: const Text('Invite'),
            ),
          ],
        ),
      ),
    );
    if (res == null || (res['user_id'] ?? '').isEmpty) return;
    try {
      await _sync.inviteMember(_book.id, res['user_id']!, res['role']!);
      if (!mounted) return;
      _toast('Invitation sent');
      _loadMembers();
    } catch (e) {
      if (!mounted) return;
      _toast('Invite failed: $e');
    }
  }

  // -- helpers ----------------------------------------------------------

  /// Run a server mutation, surface errors, and notify the parent list.
  Future<void> _mutate(Future<void> Function() op) async {
    try {
      await op();
      widget.onChanged();
    } catch (e) {
      if (!mounted) return;
      _toast('Could not save: $e');
    }
  }


  Future<String?> _dialog({
    required String title,
    required String fieldLabel,
    required TextEditingController ctrl,
    required String action,
  }) {
    return showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(title),
        content: TextField(controller: ctrl, autofocus: true, decoration: InputDecoration(labelText: fieldLabel)),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
          ElevatedButton(onPressed: () => Navigator.pop(ctx, ctrl.text), child: Text(action)),
        ],
      ),
    );
  }

  /// Run a server mutation, surface errors, and notify the parent list.
  // -- build --------------------------------------------------------

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final invited = _book.membershipStatus == 'invited';
    return DraggableScrollableSheet(
      expand: false,
      initialChildSize: 0.6,
      maxChildSize: 0.9,
      minChildSize: 0.35,
      builder: (ctx, scroll) => ListView(
        controller: scroll,
        padding: const EdgeInsets.all(16),
        children: [
          Row(
            children: [
              Icon(_tierIcons[_book.tier] ?? Icons.book, size: 32, color: theme.colorScheme.primary),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(_book.name, style: theme.textTheme.titleLarge),
                    Text(
                      '${_book.tier} - you are ${_book.yourRole}'
                      '${_book.folder.isNotEmpty ? ' - ${_book.folder}' : ''}',
                      style: theme.textTheme.bodySmall,
                    ),
                  ],
                ),
              ),
              if (_isActive)
                const Chip(label: Text('Active'))
              else if (invited)
                const Chip(label: Text('Invited')),
            ],
          ),
          const Divider(height: 24),
          if (invited) ...[
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Text('Accept this invitation to manage the Book.', style: theme.textTheme.bodySmall),
            ),
          ] else if (!_isActive)
            ListTile(
              leading: const Icon(Icons.check_circle_outline),
              title: const Text('Make active Book'),
              subtitle: const Text('Use this Book everywhere in the app'),
              onTap: _makeActive,
            ),
          ListTile(
            leading: const Icon(Icons.edit_outlined),
            title: const Text('Rename Book'),
            onTap: _rename,
          ),
          ListTile(
            leading: const Icon(Icons.notes_outlined),
            title: const Text('Description'),
            subtitle: _book.description.isEmpty ? null : Text(_book.description, maxLines: 1, overflow: TextOverflow.ellipsis),
            onTap: _editDescription,
          ),
          ListTile(
            leading: const Icon(Icons.drive_file_move_outline),
            title: Text(_book.folder.isEmpty ? 'Move to folder' : 'Folder: ${_book.folder}'),
            onTap: _moveToFolder,
          ),
          ListTile(
            leading: const Icon(Icons.sync),
            title: Text('Sync "${_book.name}" now'),
            subtitle: const Text('Push and pull this Book\'s entries'),
            onTap: _syncNow,
          ),
          if (_canManage) ...[
            const Divider(height: 24),
            Text('MEMBERS', style: theme.textTheme.labelSmall),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 8),
                child: Text(_error!, style: TextStyle(color: theme.colorScheme.error)),
              ),
            if (_membersLoading)
              const Padding(
                padding: EdgeInsets.all(12),
                child: Center(child: CircularProgressIndicator()),
              )
            else
              ..._members.map(
                (m) => ListTile(
                  dense: true,
                  leading: const Icon(Icons.person_outline),
                  title: Text(m.displayName.isEmpty ? m.userId : m.displayName),
                  subtitle: Text('${m.role} - ${m.status}'),
                ),
              ),
            ListTile(
              leading: const Icon(Icons.person_add_alt),
              title: const Text('Invite member'),
              onTap: _invite,
            ),
          ],
          const SizedBox(height: 12),
        ],
      ),
    );
  }
}
