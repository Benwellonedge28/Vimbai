import 'package:flutter/material.dart';

import '../data/book_sync_service.dart';

/// Who changed this Book, in words rather than in record names.
///
/// A Book that has been shared is kept by several people, so the first question when the numbers move unexpectedly is who
/// moved them. The engine has recorded every invitation, acceptance, device and pushed batch against the person who did it;
/// this is that record read back, with the actor named and the action said plainly.
///
/// It is shown to every member, not only to administrators, because everybody with access to the Book is affected by what the
/// others do to it, and the record holds nothing the person did not already have access to.
class BookActivitySheet extends StatefulWidget {
  const BookActivitySheet({
    super.key,
    required this.bookId,
    required this.bookName,
  });

  final String bookId;
  final String bookName;

  static Future<void> show(
    BuildContext context, {
    required String bookId,
    required String bookName,
  }) {
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (_) => BookActivitySheet(bookId: bookId, bookName: bookName),
    );
  }

  @override
  State<BookActivitySheet> createState() => _BookActivitySheetState();
}

class _BookActivitySheetState extends State<BookActivitySheet> {
  final _sync = BookSyncService.instance;
  List<Map<String, dynamic>> _rows = const [];
  Map<String, String> _names = const {};
  bool _loading = true;
  String? _error;

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
      final rows = await _sync.activity(widget.bookId);
      // the record names people by the id the engine uses; the members list is what turns those into names
      final names = <String, String>{};
      try {
        for (final member in await _sync.members(widget.bookId)) {
          if (member.displayName.isNotEmpty) {
            names[member.userId] = member.displayName;
          }
        }
      } catch (_) {
        // an audit without names is still an audit: the row falls back to naming the person by their id
      }
      if (!mounted) return;
      setState(() {
        _rows = rows;
        _names = names;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = 'Activity is unavailable offline';
      });
    }
  }

  String _who(String actor) {
    if (actor.isEmpty) return 'Somebody';
    return _names[actor] ??
        'Somebody (${actor.length > 8 ? actor.substring(0, 8) : actor})';
  }

  /// What happened, said the way a person would say it.
  String _said(Map<String, dynamic> row) {
    final actor = _who(row['actor'] as String? ?? '');
    final detail = row['detail'] as String? ?? '';
    switch (row['action'] as String? ?? '') {
      case 'book.created':
        return '$actor opened this Book';
      case 'book.updated':
        return '$actor changed the details of this Book';
      case 'member.invited':
        return '$actor invited somebody into this Book';
      case 'member.accepted':
        return '$actor joined this Book';
      case 'member.updated':
        final parts = detail.split(':');
        final person = parts.isNotEmpty ? _who(parts.first) : 'somebody';
        final status = parts.length > 2 ? parts[2] : '';
        if (status == 'revoked') {
          return '$actor took $person out of this Book';
        }
        final role = parts.length > 1 ? parts[1] : '';
        if (role.isNotEmpty) return '$actor made $person a $role';
        return '$actor changed what $person may do';
      case 'device.registered':
        return '$actor connected a device to this Book';
      case 'entries.pushed':
        return '$actor added entries from their device ($detail)';
      default:
        final action = row['action'] as String? ?? 'something';
        return '$actor did $action';
    }
  }

  String _when(Object? at) {
    final seconds = at is num ? at.toDouble() : double.tryParse('$at');
    if (seconds == null || seconds <= 0) return '';
    final when = DateTime.fromMillisecondsSinceEpoch((seconds * 1000).round());
    final now = DateTime.now();
    final sameDay =
        when.year == now.year && when.month == now.month && when.day == now.day;
    final two = (int n) => n.toString().padLeft(2, '0');
    return sameDay
        ? 'Today ${two(when.hour)}:${two(when.minute)}'
        : '${when.year}-${two(when.month)}-${two(when.day)} ${two(when.hour)}:${two(when.minute)}';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return DraggableScrollableSheet(
      expand: false,
      initialChildSize: 0.7,
      maxChildSize: 0.95,
      builder: (context, controller) => ListView(
        controller: controller,
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'What happened in "${widget.bookName}"',
                  style: theme.textTheme.titleMedium,
                ),
              ),
              IconButton(
                icon: const Icon(Icons.refresh),
                onPressed: _loading ? null : _load,
              ),
            ],
          ),
          if (_loading)
            const Padding(
              padding: EdgeInsets.all(16),
              child: Center(child: CircularProgressIndicator()),
            ),
          // a failed read must not be dressed up as a Book nothing has happened in
          if (_error != null && !_loading)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Text(
                _error!,
                style: TextStyle(color: theme.colorScheme.error),
              ),
            ),
          if (!_loading && _error == null && _rows.isEmpty)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 16),
              child: Text('Nothing has been recorded yet.'),
            ),
          if (!_loading && _error == null)
            ..._rows.map(
              (row) => ListTile(
                dense: true,
                leading: const Icon(Icons.history),
                title: Text(_said(row)),
                subtitle: Text(_when(row['created_at'])),
              ),
            ),
        ],
      ),
    );
  }
}
