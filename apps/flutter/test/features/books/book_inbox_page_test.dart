import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/books/presentation/book_inbox_page.dart';
import 'package:vimbai_mobile_client/features/multimodal/data/multimodal_api_service.dart';
import 'package:vimbai_mobile_client/features/multimodal/models/multimodal_models.dart';

/// The engine's inbox, refusing every read, so the screen has to say so rather than say the Book is empty.
class _RefusingInbox extends MultimodalApiService {
  _RefusingInbox() : super(client: null);

  @override
  Future<List<MultimodalProcessingTaskInDB>> getBookInbox({
    String view = 'incomplete',
    int limit = 100,
    int offset = 0,
  }) async => throw Exception('the engine is unreachable');

  @override
  Future<Map<String, dynamic>> getInboxSummary() async =>
      throw Exception('the engine is unreachable');
}

/// The engine's inbox answering one capture, the way it stamps one.
class _OneCaptureInbox extends MultimodalApiService {
  _OneCaptureInbox() : super(client: null);

  @override
  Future<List<MultimodalProcessingTaskInDB>> getBookInbox({
    String view = 'incomplete',
    int limit = 100,
    int offset = 0,
  }) async => [
    MultimodalProcessingTaskInDB.fromJson({
      'id': 'capture-1',
      'input_type': 'text',
      'input_raw_text': 'paid 40 for fuel',
      'status': 'received',
      'created_at': '1763300000.5',
      'updated_at': '1763300000.5',
    }),
  ];

  @override
  Future<Map<String, dynamic>> getInboxSummary() async => {
    'received': 1,
    'total_incomplete': 1,
    'total_organized': 0,
    'review_pending': 0,
  };
}

Widget _screen(MultimodalApiService api) => MaterialApp(
  home: BookInboxPage(
    api: api,
    pendingSyncCount: () async => 0,
    liveUpdates: false,
  ),
);

void main() {
  testWidgets('a read that failed never claims the inbox is empty', (
    tester,
  ) async {
    await tester.pumpWidget(_screen(_RefusingInbox()));
    await tester.pumpAndSettle();

    expect(find.textContaining('Could not load inbox'), findsOneWidget);
    expect(find.textContaining('No incomplete records'), findsNothing);
  });

  testWidgets('a capture the engine holds reads as a row', (tester) async {
    await tester.pumpWidget(_screen(_OneCaptureInbox()));
    await tester.pumpAndSettle();

    // the row exists, and the screen is not claiming a failure it never had
    expect(find.byType(Card), findsOneWidget);
    expect(find.textContaining('Could not load inbox'), findsNothing);
    expect(find.textContaining('No incomplete records'), findsNothing);
  });
}
