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

/// The engine holding one capture and refusing to accept it, the way it refuses an unread one.
class _UnreadCaptureInbox extends _OneCaptureInbox {
  @override
  Future<MultimodalProcessingTaskInDB> organizeRecord(String taskId) async =>
      throw Exception(
        'This capture has not been read yet, so there is no entry to post.',
      );
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

    expect(
      find.textContaining('This Book\'s inbox could not be read just now.'),
      findsOneWidget,
    );
    expect(find.textContaining('the engine is unreachable'), findsOneWidget);
    expect(find.textContaining('No records waiting'), findsNothing);
  });

  testWidgets('a capture the engine holds reads as a row', (tester) async {
    await tester.pumpWidget(_screen(_OneCaptureInbox()));
    await tester.pumpAndSettle();

    // the row exists, and the screen is not claiming a failure it never had
    expect(find.byType(Card), findsOneWidget);
    expect(find.textContaining('could not be read just now'), findsNothing);
    expect(find.textContaining('No records waiting'), findsNothing);
  });

  testWidgets('an unread capture says why it was not accepted, and stays', (
    tester,
  ) async {
    await tester.pumpWidget(_screen(_UnreadCaptureInbox()));
    await tester.pumpAndSettle();

    // open the record and try to accept it into the Book
    await tester.tap(find.byType(Card));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Organize into Book'));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Not organized into the Book: This capture has not'),
      findsOneWidget,
    );
    // the row is still there: a refused accept never removes the record
    expect(find.byType(Card), findsOneWidget);
  });
}
