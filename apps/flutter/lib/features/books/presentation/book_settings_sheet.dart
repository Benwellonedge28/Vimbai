// Per-Book settings sheet.
//
// Opened from the kebab (⋮) menu on each Book card. Every setting here
// applies to THIS Book only: its name, description, folder, members and
// sync. Two pages (home list + Books page) share this sheet.

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:vimbai_mobile_client/features/books/models/book_models.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';
import 'package:vimbai_mobile_client/features/books/presentation/book_activity_sheet.dart';
import 'package:vimbai_mobile_client/features/organisations/data/organisation_service.dart';
import 'package:vimbai_mobile_client/features/organisations/models/organisation_models.dart';
import 'package:vimbai_mobile_client/features/organisations/presentation/put_book_in_organisation_sheet.dart';

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
    builder: (ctx) =>
        _BookSettingsSheet(book: book, folders: folders, onChanged: onChanged),
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

  /// The organisation this Book is inside, once the server has said. [_sharingKnown] separates "not in one" from "not asked":
  /// claiming a Book is private because the server did not answer is the same lie as claiming it is shared.
  VOrganisation? _organisation;
  bool _sharingKnown = false;

  @override
  void initState() {
    super.initState();
    _book = widget.book;
    _loadActive();
    _loadSharing();
    if (_canManage) _loadMembers();
  }

  Future<void> _loadActive() async {
    final prefs = await SharedPreferences.getInstance();
    if (mounted)
      setState(() => _isActive = prefs.getString('active_book_id') == _book.id);
  }

  Future<void> _loadSharing() async {
    try {
      final organisation = await OrganisationService().ofBook(_book.id);
      if (mounted)
        setState(() {
          _organisation = organisation;
          _sharingKnown = true;
        });
    } catch (_) {
      // Quietly: the row then says nothing about sharing rather than guessing.
    }
  }

  Future<void> _changeSharing() async {
    final moved = await PutBookInOrganisationSheet.show(
      context,
      bookId: _book.id,
      bookName: _book.name,
      organisationId: _organisation?.id,
    );
    if (moved && mounted) await _loadSharing();
  }

  Future<void> _loadMembers() async {
    setState(() => _membersLoading = true);
    try {
      final ms = await _sync.members(_book.id);
      if (mounted)
        setState(() {
          _members = ms;
          _membersLoading = false;
          _error = null;
        });
    } catch (e) {
      if (mounted)
        setState(() {
          _membersLoading = false;
          _error = 'Members unavailable offline';
        });
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
    if (name == null || name.trim().isEmpty || name.trim() == _book.name)
      return;
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
          title: Text(
            _book.folder.isEmpty ? 'Move to folder' : 'Change folder',
          ),
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
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('Cancel'),
            ),
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
      BookContext.instance.setBook(
        BookContextBook(
          id: _book.id,
          name: _book.name,
          tier: _book.tier,
          yourRole: _book.yourRole,
          source: 'sync',
        ),
      );
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
      _toast(
        'Synced "${_book.name}": pushed ${r['pushed'] ?? 0}, pulled ${r['pulled'] ?? 0}',
      );
    } catch (e) {
      if (!mounted) return;
      _toast('Sync failed: $e');
    }
  }

  /// Delete a Book that was made by mistake, after asking once, and carrying the engine's own reason if it refuses.
  Future<void> _delete() async {
    final sure = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Delete "${_book.name}"?'),
        content: const Text(
          'This cannot be undone. A Book with postings, or with other people '
          'in it, will not be deleted.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('Keep it'),
          ),
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(true),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    if (sure != true || !mounted) return;
    try {
      await _sync.deleteBook(_book.id);
      if (!mounted) return;
      Navigator.of(context).pop();
      _toast('Deleted "${_book.name}"');
    } catch (e) {
      if (!mounted) return;
      _toast('$e');
    }
  }

  /// Leave a Book somebody else owns, after asking once.
  Future<void> _leave() async {
    final sure = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Leave "${_book.name}"?'),
        content: const Text(
          'You will stop seeing this Book, and nothing you post will reach it. '
          'What is already on this phone can stay.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('Stay'),
          ),
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(true),
            child: const Text('Leave'),
          ),
        ],
      ),
    );
    if (sure != true || !mounted) return;
    try {
      await _sync.leaveBook(_book.id);
      if (!mounted) return;
      Navigator.of(context).pop();
      _toast('You left "${_book.name}"');
    } catch (e) {
      if (!mounted) return;
      _toast('Could not leave the Book: $e');
    }
  }

  /// Change what a member may do, or take them out of the Book.
  Future<void> _manageMember(BookMember member) async {
    final who = member.displayName.isEmpty ? member.userId : member.displayName;
    final choice = await showDialog<String>(
      context: context,
      builder: (ctx) => SimpleDialog(
        title: Text(who),
        children: [
          SimpleDialogOption(
            onPressed: () => Navigator.of(ctx).pop('viewer'),
            child: const Text('Change to viewer (can read)'),
          ),
          SimpleDialogOption(
            onPressed: () => Navigator.of(ctx).pop('bookkeeper'),
            child: const Text('Change to bookkeeper (can post)'),
          ),
          SimpleDialogOption(
            onPressed: () => Navigator.of(ctx).pop('admin'),
            child: const Text('Change to admin (can invite others)'),
          ),
          SimpleDialogOption(
            onPressed: () => Navigator.of(ctx).pop('revoked'),
            child: const Text('Take out of this Book'),
          ),
        ],
      ),
    );
    if (choice == null || !mounted) return;
    final taking_out = choice == 'revoked';
    try {
      await _sync.updateMember(
        _book.id,
        member.userId,
        role: taking_out ? null : choice,
        status: taking_out ? 'revoked' : null,
      );
      if (!mounted) return;
      // say what happened in the words of the thing that happened: taking somebody out is not a role change
      _toast(
        taking_out
            ? '$who can no longer open this Book'
            : '$who is now a $choice',
      );
      await _loadMembers();
    } catch (e) {
      if (!mounted) return;
      _toast('Could not change $who: $e');
    }
  }

  Future<void> _invite() async {
    final nameCtrl = TextEditingController();
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
                controller: nameCtrl,
                decoration: const InputDecoration(
                  labelText: 'Username or email',
                  helperText: 'The name they signed up with',
                ),
              ),
              DropdownButton<String>(
                value: role,
                isExpanded: true,
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
              onPressed: () => Navigator.pop(ctx, {
                'username': nameCtrl.text.trim(),
                'role': role,
              }),
              child: const Text('Invite'),
            ),
          ],
        ),
      ),
    );
    if (res == null || (res['username'] ?? '').isEmpty) return;
    try {
      final person = await _sync.inviteMemberByUsername(
        _book.id,
        res['username']!,
        res['role']!,
      );
      if (!mounted) return;
      final name = (person['full_name'] as String?)?.trim();
      _toast(
        'Invitation sent to ${name == null || name.isEmpty ? person['username'] : name}',
      );
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
        content: TextField(
          controller: ctrl,
          autofocus: true,
          decoration: InputDecoration(labelText: fieldLabel),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(ctx, ctrl.text),
            child: Text(action),
          ),
        ],
      ),
    );
  }

  /// Run a server mutation, surface errors, and notify the parent list.
  // -- build --------------------------------------------------------

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final invited = _book.isPendingInvitation;
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
              Icon(
                _tierIcons[_book.tier] ?? Icons.book,
                size: 32,
                color: theme.colorScheme.primary,
              ),
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
              child: Text(
                'Accept this invitation to manage the Book.',
                style: theme.textTheme.bodySmall,
              ),
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
            subtitle: _book.description.isEmpty
                ? null
                : Text(
                    _book.description,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
            onTap: _editDescription,
          ),
          ListTile(
            leading: const Icon(Icons.drive_file_move_outline),
            title: Text(
              _book.folder.isEmpty
                  ? 'Move to folder'
                  : 'Folder: ${_book.folder}',
            ),
            onTap: _moveToFolder,
          ),
          ListTile(
            leading: Icon(
              _organisation == null
                  ? Icons.lock_outline
                  : Icons.business_outlined,
            ),
            title: const Text('Sharing'),
            subtitle: !_sharingKnown
                ? null
                : Text(
                    _organisation == null
                        ? 'Only you can see this Book. Put it in an organisation to share it.'
                        : 'In ${_organisation!.name}',
                  ),
            onTap: _changeSharing,
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
                child: Text(
                  _error!,
                  style: TextStyle(color: theme.colorScheme.error),
                ),
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
                  // the owner's own row is not editable through the engine, and a sheet that offered it would only produce
                  // a refusal; the same goes for a Book this person cannot administer
                  trailing: m.role == 'owner' || !_canManage
                      ? null
                      : const Icon(Icons.more_vert),
                  onTap: m.role == 'owner' || !_canManage
                      ? null
                      : () => _manageMember(m),
                ),
              ),
            ListTile(
              leading: const Icon(Icons.person_add_alt),
              title: const Text('Invite member'),
              onTap: _invite,
            ),
            ListTile(
              leading: const Icon(Icons.history),
              title: const Text('What happened in this Book'),
              subtitle: const Text('Who changed what, and when'),
              onTap: () => BookActivitySheet.show(
                context,
                bookId: _book.id,
                bookName: _book.name,
              ),
            ),
            // a Book made by mistake, or made at the wrong tier, has nowhere else to go: every other setting here can be
            // changed and the Book itself could not be taken away
            if (_book.yourRole == 'owner')
              ListTile(
                leading: Icon(
                  Icons.delete_outline,
                  color: Theme.of(context).colorScheme.error,
                ),
                title: Text(
                  'Delete this Book',
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
                subtitle: const Text(
                  'Only an empty Book can go. Once it has postings, it stays.',
                ),
                onTap: _delete,
              ),
            // the person's own way out: being taken out is the owner's side of it, and waiting for them is not always
            // possible when the reason for leaving is the owner
            if (_book.yourRole != 'owner' &&
                _book.membershipStatus != 'revoked')
              ListTile(
                leading: Icon(
                  Icons.logout,
                  color: Theme.of(context).colorScheme.error,
                ),
                title: Text(
                  'Leave this Book',
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
                subtitle: const Text(
                  'Stop being in it. Entries already on this phone are kept.',
                ),
                onTap: _leave,
              ),
          ],
          const SizedBox(height: 12),
        ],
      ),
    );
  }
}
