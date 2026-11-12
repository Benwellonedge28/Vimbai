// Create Book wizard - the single entry point for starting a new Book.
//
// Flow: Start (intro) -> choose the Book's category (audience tier) ->
// name it (with optional folder) -> Create. The created Book immediately
// becomes the active Book context so every service call after creation
// runs inside the new Book.

import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/services/book_sync_service.dart';

class _TierChoice {
  final String id;
  final String title;
  final String blurb;
  final IconData icon;
  const _TierChoice(this.id, this.title, this.blurb, this.icon);
}

const List<_TierChoice> _tiers = [
  _TierChoice(
    'personal',
    'Personal',
    'My own money. Just me - private by default.',
    Icons.person,
  ),
  _TierChoice(
    'household',
    'Household',
    'Family budgeting and shared home expenses.',
    Icons.home,
  ),
  _TierChoice(
    'group',
    'Group',
    'Stokvels, savings clubs and societies.',
    Icons.groups,
  ),
  _TierChoice(
    'business',
    'Business',
    'Full accounting for a company or tuckshop.',
    Icons.business_center,
  ),
  _TierChoice(
    'nonprofit',
    'Nonprofit',
    'Fund accounting, donors and grants reporting.',
    Icons.volunteer_activism,
  ),
];

class CreateBookWizardPage extends StatefulWidget {
  const CreateBookWizardPage({super.key});

  @override
  State<CreateBookWizardPage> createState() => _CreateBookWizardPageState();
}

class _CreateBookWizardPageState extends State<CreateBookWizardPage> {
  final BookSyncService _sync = BookSyncService.instance;
  final TextEditingController _nameCtrl = TextEditingController();
  final TextEditingController _descriptionCtrl = TextEditingController();

  int _step = 0; // 0 = start, 1 = category, 2 = name, 3 = creating
  String? _tier;
  String _folder = ''; // '' = unfiled
  List<String> _existingFolders = const [];
  bool _creating = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _loadFolders();
  }

  Future<void> _loadFolders() async {
    try {
      final books = await _sync.localBooks();
      final folders = books
          .map((b) => b.folder)
          .where((f) => f.isNotEmpty)
          .toSet()
          .toList()
        ..sort();
      if (mounted) setState(() => _existingFolders = folders);
    } catch (_) {
      // folder suggestions are best-effort
    }
  }

  void _back() {
    if (_step == 0 || _creating) return;
    setState(() {
      _step--;
      _error = null;
    });
  }

  void _chooseTier(String tier) {
    setState(() {
      _tier = tier;
      _step = 2;
    });
  }

  Future<void> _create() async {
    final name = _nameCtrl.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Give your Book a name');
      return;
    }
    setState(() {
      _creating = true;
      _error = null;
      _step = 3;
    });
    try {
      final book = await _sync.createBook(
        name,
        _tier!,
        description: _descriptionCtrl.text.trim(),
        folder: _folder,
      );
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('"${book.name}" is ready')),
        );
        // pop with the id so the Books page makes it the active context
        Navigator.of(context).pop(book.id);
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _creating = false;
          _step = 2;
          _error = 'Could not create the Book: $e';
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Start a Book'),
        automaticallyImplyLeading: !_creating,
        leading: _step == 0 || _creating
            ? null
            : BackButton(onPressed: _back),
      ),
      body: SafeArea(
        child: _step == 0
            ? _buildStart()
            : _step == 1
                ? _buildCategoryStep()
                : _buildNameStep(),
      ),
    );
  }

  // -- step 0: intro -------------------------------------------------

  Widget _buildStart() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(28),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              Icons.menu_book,
              size: 84,
              color: Theme.of(context).colorScheme.primary,
            ),
            const SizedBox(height: 24),
            const Text(
              'Every part of Vimbai lives inside a Book - '
              'your personal money, a household, a stokvel, a business '
              'or a nonprofit.',
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 12),
            Text(
              'You choose the category, give it a name, and it becomes '
              'its own private space with its own accounts.',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 36),
            SizedBox(
              width: double.infinity,
              child: FilledButton.icon(
                icon: const Icon(Icons.add),
                label: const Text('Start a Book'),
                onPressed: () => setState(() => _step = 1),
              ),
            ),
          ],
        ),
      ),
    );
  }

  // -- step 1: category ----------------------------------------------

  Widget _buildCategoryStep() {
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text(
          'Choose the category',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: 4),
        Text(
          'This decides which features the Book unlocks.',
          style: Theme.of(context).textTheme.bodySmall,
        ),
        const SizedBox(height: 16),
        ..._tiers.map(
          (t) => Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Card(
              child: ListTile(
                leading: Icon(t.icon),
                title: Text(t.title),
                subtitle: Text(t.blurb),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => _chooseTier(t.id),
              ),
            ),
          ),
        ),
      ],
    );
  }

  // -- step 2: name ---------------------------------------------------

  Widget _buildNameStep() {
    final tierChoice = _tiers.firstWhere((t) => t.id == _tier);
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Row(
          children: [
            Icon(tierChoice.icon),
            const SizedBox(width: 8),
            Text(
              tierChoice.title,
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ],
        ),
        const SizedBox(height: 20),
        Text(
          'Name your Book',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _nameCtrl,
          autofocus: true,
          maxLength: 120,
          decoration: const InputDecoration(
            labelText: 'Book name',
            hintText: 'e.g. Mukomana Household',
            border: OutlineInputBorder(),
          ),
          onSubmitted: (_) => _create(),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: _descriptionCtrl,
          maxLength: 200,
          decoration: const InputDecoration(
            labelText: 'Description (optional)',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: 20),
        Text(
          'Folder (optional)',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        Text(
          'Keep your Books organized. You can move Books between folders anytime.',
          style: Theme.of(context).textTheme.bodySmall,
        ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            ChoiceChip(
              label: const Text('No folder'),
              selected: _folder.isEmpty,
              onSelected: (_) => setState(() => _folder = ''),
            ),
            ..._existingFolders.map(
              (f) => ChoiceChip(
                label: Text(f),
                selected: _folder == f,
                onSelected: (_) => setState(() => _folder = f),
              ),
            ),
            ActionChip(
              avatar: const Icon(Icons.create_new_folder_outlined, size: 18),
              label: const Text('New folder'),
              onPressed: () => _newFolderDialog(),
            ),
          ],
        ),
        if (_error != null) ...[
          const SizedBox(height: 16),
          Text(
            _error!,
            style: TextStyle(color: Theme.of(context).colorScheme.error),
          ),
        ],
        const SizedBox(height: 24),
        _creating
            ? const Center(child: CircularProgressIndicator())
            : FilledButton.icon(
                icon: const Icon(Icons.check),
                label: const Text('Create the Book'),
                onPressed: _create,
              ),
      ],
    );
  }

  Future<void> _newFolderDialog() async {
    final ctrl = TextEditingController();
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('New folder'),
        content: TextField(
          controller: ctrl,
          autofocus: true,
          maxLength: 120,
          decoration: const InputDecoration(labelText: 'Folder name'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(ctx, ctrl.text.trim()),
            child: const Text('Add'),
          ),
        ],
      ),
    );
    if (name == null || name.isEmpty) return;
    setState(() {
      if (!_existingFolders.contains(name)) {
        _existingFolders = [..._existingFolders, name]..sort();
      }
      _folder = name;
    });
  }
}
