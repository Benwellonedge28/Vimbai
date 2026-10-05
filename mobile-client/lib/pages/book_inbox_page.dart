import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';
import 'package:vimbai_mobile_client/models/multimodal_models.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';
import 'package:vimbai_mobile_client/services/multimodal_api_service.dart';

/// The Book Inbox: incomplete records wired to this Book.
///
/// Users capture physical records with the camera (photo, video or
/// continuous scanning); Vimbai processes everything in the background
/// and the results appear here, ready to be organized into the Book.
class BookInboxPage extends StatefulWidget {
  const BookInboxPage({super.key});

  @override
  State<BookInboxPage> createState() => _BookInboxPageState();
}

class _BookInboxPageState extends State<BookInboxPage> {
  final MultimodalApiService _api = MultimodalApiService();
  final ImagePicker _picker = ImagePicker();

  String _view = 'incomplete'; // incomplete | organized | all
  Map<String, dynamic> _summary = {};
  List<MultimodalProcessingTaskInDB> _records = [];
  bool _loading = false;
  String? _error;
  Timer? _autoRefresh;

  @override
  void initState() {
    super.initState();
    _refresh();
    // Records move through statuses in the background; keep the list live.
    _autoRefresh = Timer.periodic(const Duration(seconds: 5), (_) => _refresh());
  }

  @override
  void dispose() {
    _autoRefresh?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    setState(() {
      _loading = _records.isEmpty;
      _error = null;
    });
    try {
      final summary = await _api.getInboxSummary();
      final records = await _api.getBookInbox(view: _view);
      if (!mounted) return;
      setState(() {
        _summary = summary;
        _records = records;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.toString();
        _loading = false;
      });
    }
  }

  Future<void> _setView(String view) async {
    _view = view;
    await _refresh();
  }

  // ------------------------------------------------------------------
  // Capture actions (fire-and-forget: Vimbai handles the rest)
  // ------------------------------------------------------------------

  Future<void> _capturePhoto() async {
    try {
      final file = await _picker.pickImage(source: ImageSource.camera);
      if (file == null) return;
      await _api.capturePhoto(File(file.path));
      _toast('Photo captured - Vimbai is processing it in the background');
      _refresh();
    } catch (e) {
      _toast('Capture failed: $e');
    }
  }

  Future<void> _captureVideo() async {
    try {
      final file = await _picker.pickVideo(source: ImageSource.camera);
      if (file == null) return;
      await _api.captureVideo(File(file.path));
      _toast('Video captured - Vimbai is processing it in the background');
      _refresh();
    } catch (e) {
      _toast('Capture failed: $e');
    }
  }

  /// Continuous scanning: keep taking photos of physical records until the
  /// user taps Done. Every page is submitted immediately and processed in
  /// the background.
  Future<void> _continuousScan() async {
    var scanning = true;
    var count = 0;
    while (scanning) {
      final file = await _picker.pickImage(source: ImageSource.camera);
      if (file == null) break; // camera dismissed
      try {
        await _api.capturePhoto(File(file.path), sourceContext: 'continuous_scan');
        count++;
      } catch (e) {
        _toast('Scan failed: $e');
      }
      scanning = await _showScanNextDialog(count);
    }
    if (count > 0) {
      _toast('$count page(s) scanned - processing in the background');
      _refresh();
    }
  }

  Future<bool> _showScanNextDialog(int scanned) async {
    final next = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Continuous scan'),
        content: Text('$scanned page(s) captured so far. Scan another page?'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Done'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Scan next'),
          ),
        ],
      ),
    );
    return next ?? false;
  }

  Future<void> _captureTextNote() async {
    final controller = TextEditingController();
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('New note'),
        content: TextField(
          controller: controller,
          maxLines: 4,
          autofocus: true,
          decoration: const InputDecoration(hintText: 'e.g. Paid school fees 200 USD'),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
          FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Capture')),
        ],
      ),
    );
    if (confirmed != true || controller.text.trim().isEmpty) return;
    try {
      await _api.captureText(controller.text.trim());
      _toast('Note captured - Vimbai is processing it in the background');
      _refresh();
    } catch (e) {
      _toast('Capture failed: $e');
    }
  }

  // ------------------------------------------------------------------
  // Record actions
  // ------------------------------------------------------------------

  Future<void> _organize(MultimodalProcessingTaskInDB record) async {
    try {
      await _api.organizeRecord(record.id);
      _toast('Organized into the Book');
      _refresh();
    } catch (e) {
      _toast('Could not organize: $e');
    }
  }

  Future<void> _discard(MultimodalProcessingTaskInDB record) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Discard record?'),
        content: const Text('This captured record will be removed from the Book inbox.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Keep')),
          FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Discard')),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await _api.deleteTask(record.id);
      _refresh();
    } catch (e) {
      _toast('Could not discard: $e');
    }
  }

  void _showRecordDetails(MultimodalProcessingTaskInDB record) {
    final sje = record.suggestedJournalEntry;
    final doc = record.documentResult;
    final audio = record.audioResult;
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      builder: (ctx) => SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(_inputTypeLabel(record.inputType),
                  style: Theme.of(ctx).textTheme.titleLarge),
              const SizedBox(height: 4),
              Text('Status: ${_statusLabel(record.status)}'),
              const SizedBox(height: 12),
              if (doc?.rawText != null && doc!.rawText!.isNotEmpty) ...[
                Text('Extracted text', style: Theme.of(ctx).textTheme.titleSmall),
                const SizedBox(height: 4),
                Text(doc.rawText!),
                const SizedBox(height: 12),
              ],
              if (audio?.transcribedText != null) ...[
                Text('Transcribed text', style: Theme.of(ctx).textTheme.titleSmall),
                const SizedBox(height: 4),
                Text(audio!.transcribedText!),
                const SizedBox(height: 12),
              ],
              if (doc != null && doc.extractedData.isNotEmpty) ...[
                Text('Extracted fields', style: Theme.of(ctx).textTheme.titleSmall),
                const SizedBox(height: 4),
                ...doc.extractedData.map(
                  (f) => Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        Expanded(child: Text(f.name)),
                        Text(f.value, textAlign: TextAlign.end),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 12),
              ],
              if (sje != null) ...[
                Text('Suggested record', style: Theme.of(ctx).textTheme.titleSmall),
                const SizedBox(height: 4),
                ...sje.entries.map(
                  (e) => Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        Expanded(child: Text(e.key)),
                        Text('${e.value}', textAlign: TextAlign.end),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 12),
              ],
              if (record.errors.isNotEmpty) ...[
                Text('Errors', style: Theme.of(ctx).textTheme.titleSmall),
                ...record.errors.map((e) => Text(e)),
                const SizedBox(height: 12),
              ],
              Row(
                children: [
                  Expanded(
                    child: OutlinedButton(
                      onPressed: () {
                        Navigator.pop(ctx);
                        _discard(record);
                      },
                      child: const Text('Discard'),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: FilledButton(
                      onPressed: record.status == MultimodalProcessingStatus.completed
                          ? null
                          : () {
                              Navigator.pop(ctx);
                              _organize(record);
                            },
                      child: const Text('Organize into Book'),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  void _toast(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message), duration: const Duration(seconds: 3)),
    );
  }

  // ------------------------------------------------------------------
  // UI helpers
  // ------------------------------------------------------------------

  String _inputTypeLabel(MultimodalInputType t) => switch (t) {
        MultimodalInputType.image => 'Photo capture',
        MultimodalInputType.video => 'Video capture',
        MultimodalInputType.audio => 'Voice note',
        MultimodalInputType.document => 'Document',
        MultimodalInputType.text => 'Typed note',
      };

  String _statusLabel(MultimodalProcessingStatus s) => switch (s) {
        MultimodalProcessingStatus.received => 'Received',
        MultimodalProcessingStatus.processing => 'Processing',
        MultimodalProcessingStatus.aiExtracted => 'AI extracted',
        MultimodalProcessingStatus.reviewPending => 'Needs review',
        MultimodalProcessingStatus.userCorrected => 'Corrected',
        MultimodalProcessingStatus.completed => 'Organized',
        MultimodalProcessingStatus.failed => 'Failed',
      };

  IconData _inputTypeIcon(MultimodalInputType t) => switch (t) {
        MultimodalInputType.image => Icons.photo_camera,
        MultimodalInputType.video => Icons.videocam,
        MultimodalInputType.audio => Icons.mic,
        MultimodalInputType.document => Icons.description,
        MultimodalInputType.text => Icons.notes,
      };

  Color _statusColor(MultimodalProcessingStatus s) => switch (s) {
        MultimodalProcessingStatus.received => Colors.blueGrey,
        MultimodalProcessingStatus.processing => Colors.blue,
        MultimodalProcessingStatus.aiExtracted => Colors.indigo,
        MultimodalProcessingStatus.reviewPending => Colors.orange,
        MultimodalProcessingStatus.userCorrected => Colors.teal,
        MultimodalProcessingStatus.completed => Colors.green,
        MultimodalProcessingStatus.failed => Colors.red,
      };

  @override
  Widget build(BuildContext context) {
    final book = BookContext.instance.current;
    return Scaffold(
      appBar: AppBar(
        title: Text(book != null ? '${book.name} inbox' : 'Book inbox'),
        actions: [
          IconButton(onPressed: _refresh, icon: const Icon(Icons.refresh)),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : RefreshIndicator(
              onRefresh: _refresh,
              child: Column(
                children: [
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.all(12),
                      child: Text('Could not load inbox: $_error',
                          style: TextStyle(color: Theme.of(context).colorScheme.error)),
                    ),
                  _summaryBar(),
                  SegmentedButton<String>(
                    segments: const [
                      ButtonSegment(value: 'incomplete', label: Text('Incomplete')),
                      ButtonSegment(value: 'organized', label: Text('Organized')),
                      ButtonSegment(value: 'all', label: Text('All')),
                    ],
                    selected: {_view},
                    onSelectionChanged: (s) => _setView(s.first),
                  ),
                  Expanded(child: _recordsList()),
                ],
              ),
            ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _showCaptureSheet,
        icon: const Icon(Icons.photo_camera),
        label: const Text('Capture'),
      ),
    );
  }

  Widget _summaryBar() {
    final incomplete = (_summary['total_incomplete'] as int?) ?? 0;
    final organized = (_summary['total_organized'] as int?) ?? 0;
    final needsReview = (_summary['review_pending'] as int?) ?? 0;
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Row(
        children: [
          _summaryChip('Incomplete', incomplete, Colors.orange),
          const SizedBox(width: 8),
          _summaryChip('Needs review', needsReview, Colors.amber.shade800),
          const SizedBox(width: 8),
          _summaryChip('Organized', organized, Colors.green),
        ],
      ),
    );
  }

  Widget _summaryChip(String label, int count, Color color) {
    return Expanded(
      child: Container(
        padding: const EdgeInsets.symmetric(vertical: 10),
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.12),
          borderRadius: BorderRadius.circular(12),
        ),
        child: Column(
          children: [
            Text('$count',
                style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold, color: color)),
            Text(label, style: TextStyle(fontSize: 11, color: color)),
          ],
        ),
      ),
    );
  }

  Widget _recordsList() {
    if (_records.isEmpty) {
      return ListView(
        children: [
          const SizedBox(height: 80),
          Icon(Icons.inbox, size: 56, color: Colors.grey.shade400),
          const SizedBox(height: 12),
          Center(
            child: Text(
              _view == 'incomplete'
                  ? 'No incomplete records.\nCapture anything with the camera and\nVimbai will organize it for this Book.'
                  : 'Nothing here yet.',
              textAlign: TextAlign.center,
              style: TextStyle(color: Colors.grey.shade600),
            ),
          ),
        ],
      );
    }
    return ListView.builder(
      padding: const EdgeInsets.only(bottom: 88),
      itemCount: _records.length,
      itemBuilder: (ctx, i) {
        final r = _records[i];
        final sje = r.suggestedJournalEntry;
        return Card(
          margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
          child: ListTile(
            leading: CircleAvatar(
              backgroundColor: _statusColor(r.status).withValues(alpha: 0.15),
              child: Icon(_inputTypeIcon(r.inputType),
                  color: _statusColor(r.status)),
            ),
            title: Text(
              sje?['description'] as String? ??
                  '${_inputTypeLabel(r.inputType)} - ${_statusLabel(r.status)}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            subtitle: Text(
              sje != null
                  ? '${sje['description']} - ${sje['amount'] ?? ''} ${sje['date'] != null ? '(${sje['date']})' : ''}'
                  : _statusLabel(r.status),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
            trailing: Chip(
              label: Text(_statusLabel(r.status), style: const TextStyle(fontSize: 11)),
              backgroundColor: _statusColor(r.status).withValues(alpha: 0.12),
              visualDensity: VisualDensity.compact,
            ),
            onTap: () => _showRecordDetails(r),
          ),
        );
      },
    );
  }

  Future<void> _showCaptureSheet() async {
    await showModalBottomSheet<void>(
      context: context,
      builder: (ctx) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const SizedBox(height: 8),
            ListTile(
              leading: const Icon(Icons.photo_camera),
              title: const Text('Take a photo'),
              subtitle: const Text('Receipt, invoice, statement page...'),
              onTap: () {
                Navigator.pop(ctx);
                _capturePhoto();
              },
            ),
            ListTile(
              leading: const Icon(Icons.videocam),
              title: const Text('Record a video'),
              subtitle: const Text('Multiple pages or a bulky record'),
              onTap: () {
                Navigator.pop(ctx);
                _captureVideo();
              },
            ),
            ListTile(
              leading: const Icon(Icons.document_scanner),
              title: const Text('Continuous scan'),
              subtitle: const Text('Keep photographing pages until done'),
              onTap: () {
                Navigator.pop(ctx);
                _continuousScan();
              },
            ),
            ListTile(
              leading: const Icon(Icons.notes),
              title: const Text('Type a note'),
              subtitle: const Text('Quick note for Vimbai to organize'),
              onTap: () {
                Navigator.pop(ctx);
                _captureTextNote();
              },
            ),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }
}
