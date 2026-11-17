import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/core/widgets/could_not_read.dart';
import 'package:vimbai_mobile_client/features/organisations/data/organisation_service.dart';
import 'package:vimbai_mobile_client/features/organisations/models/organisation_models.dart';

/// Ask which organisation a Book should go into, and put it there.
///
/// Shown once a Book exists (right after it is made, and from the Book's own settings). It exists as its own screen so the
/// two places a person can arrive from behave the same way: the same list, the same refusal wording, the same "not now".
///
/// It is deliberately not the place a Book is created: a Book belongs on the device whether or not a server can be reached,
/// so a failure here leaves the Book where it is rather than undoing it.
class PutBookInOrganisationSheet extends StatefulWidget {
  const PutBookInOrganisationSheet({
    super.key,
    required this.bookId,
    required this.bookName,
    this.organisationId,
    this.service,
  });

  final String bookId;
  final String bookName;

  /// The organisation the Book is in now, when it is in one. Only then is leaving offered: "make private" for a Book that is
  /// already private is a button that does nothing dressed as a choice.
  final String? organisationId;

  /// Handed in by a test; the app never passes one.
  final OrganisationService? service;

  /// Returns true when the Book was put in an organisation.
  static Future<bool> show(
    BuildContext context, {
    required String bookId,
    required String bookName,
    String? organisationId,
    OrganisationService? service,
  }) async {
    final put = await showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      builder: (_) => PutBookInOrganisationSheet(
        bookId: bookId,
        bookName: bookName,
        organisationId: organisationId,
        service: service,
      ),
    );
    return put ?? false;
  }

  @override
  State<PutBookInOrganisationSheet> createState() =>
      _PutBookInOrganisationSheetState();
}

class _PutBookInOrganisationSheetState
    extends State<PutBookInOrganisationSheet> {
  late final OrganisationService _service =
      widget.service ?? OrganisationService();

  bool _loading = true;
  bool _busy = false;
  String? _error;
  List<VOrganisation> _mine = const [];

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final all = await _service.mine();
      // Only the ones this person can put a Book into: a plain member would be refused, and offering a refusal is a lie
      // dressed as an option.
      final manageable = all.where((o) => o.canManageBooks).toList();
      if (mounted) {
        setState(() {
          _mine = manageable;
          _loading = false;
        });
      }
    } on OrganisationException catch (e) {
      if (mounted) {
        setState(() {
          _error = e.message;
          _loading = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _error =
              'Your organisations could not be read: ${CouldNotRead.reasonOf(e)}';
          _loading = false;
        });
      }
    }
  }

  Future<void> _makePrivate() async {
    final organisationId = widget.organisationId;
    if (organisationId == null) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await _service.makePrivate(organisationId, widget.bookId);
      if (mounted) Navigator.of(context).pop(true);
    } on OrganisationException catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _error = e.message;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _error =
              'Your organisations could not be read: ${CouldNotRead.reasonOf(e)}';
        });
      }
    }
  }

  Future<void> _choose(VOrganisation organisation) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await _service.moveBook(organisation.id, widget.bookId);
      if (mounted) Navigator.of(context).pop(true);
    } on OrganisationException catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _error = e.message;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _error =
              'Your organisations could not be read: ${CouldNotRead.reasonOf(e)}';
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Share ${widget.bookName}?',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 4),
            Text(
              'A Book only becomes shareable once it is inside an organisation. '
              'You can also do this later from the organisation page.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            if (widget.organisationId != null) ...[
              ListTile(
                contentPadding: EdgeInsets.zero,
                leading: const Icon(Icons.lock_outline),
                title: const Text('Keep this Book private'),
                subtitle: const Text('Take it out of the organisation'),
                enabled: !_busy,
                onTap: _busy ? null : _makePrivate,
              ),
              const Divider(),
            ],
            if (_loading)
              const Center(
                child: Padding(
                  padding: EdgeInsets.all(24),
                  child: CircularProgressIndicator(),
                ),
              )
            else if (_mine.isEmpty && _error == null)
              const Text(
                'You have no organisation to put it in yet. Open one from the '
                'organisation page and come back to this Book.',
              )
            else
              ..._mine.map(
                (organisation) => ListTile(
                  contentPadding: EdgeInsets.zero,
                  leading: const Icon(Icons.business_outlined),
                  title: Text(organisation.name),
                  subtitle: Text(organisation.role),
                  enabled: !_busy,
                  onTap: _busy ? null : () => _choose(organisation),
                ),
              ),
            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(
                _error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            const SizedBox(height: 12),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _busy
                      ? null
                      : () => Navigator.of(context).pop(false),
                  child: const Text('Not now'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
