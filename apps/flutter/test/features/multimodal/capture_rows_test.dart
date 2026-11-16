// What a capture has to do for the phone: arrive in the inbox, be deletable, and take a correction.
//
// Each payload here is the engine's own, written the way `capture.rs` writes it - a bare array from `/tasks` and `/inbox`,
// the engine's snake_case names, and a time stamped as epoch seconds. The generated parsers read camelCase and
// `DateTime.parse`, so every one of these threw until the names and the moment were translated. The point of these tests
// is that the translation happens once, on the way in, and that a correction is sent the way the engine reads it.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/multimodal/data/multimodal_api_service.dart';
import 'package:vimbai_mobile_client/features/multimodal/models/multimodal_models.dart';

/// A capture as the engine stores one.
Map<String, dynamic> engineRow() => {
  'id': 'capture-1',
  'input_type': 'text',
  'input_raw_text': 'paid 40 for fuel',
  'status': 'received',
  'created_at': 1763300000.5,
  'updated_at': 1763300060.25,
  'metadata': {'source_context': 'mobile'},
};

void main() {
  final originalTransport = ApiClient.transport;

  setUp(() {
    // the service reads the token from local storage, so the store is stubbed rather than absent
    TestWidgetsFlutterBinding.ensureInitialized();
    SharedPreferences.setMockInitialValues({});
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  MultimodalApiService service() => MultimodalApiService(
    baseUrl: 'https://engine.test/multimodal',
    remember: (key, body) async {},
  );

  group('a capture the engine sends', () {
    test('reads as a row, with its time', () {
      final row = MultimodalProcessingTaskInDB.fromJson(engineRow());

      expect(row.id, 'capture-1');
      expect(row.inputType, MultimodalInputType.text);
      expect(row.inputRawText, 'paid 40 for fuel');
      expect(row.status, MultimodalProcessingStatus.received);
      expect(row.createdAt.isAfter(DateTime.utc(2025)), isTrue);
      expect(
        row.userId,
        isEmpty,
        reason: 'the engine stamps no user on a capture',
      );
    });

    test('keeps a payload already written in camelCase', () {
      final row = MultimodalProcessingTaskInDB.fromJson({
        'id': 'local-1',
        'userId': 'self',
        'inputType': 'image',
        'status': 'review_pending',
        'createdAt': '2026-11-16T09:00:00.000Z',
        'updatedAt': '2026-11-16T09:00:00.000Z',
      });

      expect(row.userId, 'self');
      expect(row.inputType, MultimodalInputType.image);
      expect(row.createdAt, DateTime.utc(2026, 11, 16, 9));
    });
  });

  test(
    'a note taken on the phone is written the way the engine reads it',
    () async {
      late Map<String, dynamic> sent;
      ApiClient.transport = MockClient((request) async {
        sent = jsonDecode(request.body) as Map<String, dynamic>;
        return http.Response(
          jsonEncode({
            ...engineRow(),
            'id': 'capture-2',
            'input_raw_text': 'paid 40 for fuel',
          }),
          200,
          headers: {'content-type': 'application/json'},
        );
      });

      final kept = await service().createTask(
        MultimodalProcessingTaskCreate(
          userId: 'self',
          inputType: MultimodalInputType.text,
          inputRawText: 'paid 40 for fuel',
          metadata: {'source_context': 'mobile'},
        ),
      );

      // the engine refuses a capture whose kind it cannot name, so the kind has to arrive as `input_type`
      expect(sent['input_type'], 'text');
      expect(sent['input_raw_text'], 'paid 40 for fuel');
      expect(sent.containsKey('inputType'), isFalse);
      expect(
        sent.containsKey('userId'),
        isFalse,
        reason: 'a capture belongs to the Book it came through',
      );
      expect(kept.id, 'capture-2');
    },
  );

  test('the inbox reads the bare array the engine answers', () async {
    late Uri asked;
    ApiClient.transport = MockClient((request) async {
      asked = request.url;
      return http.Response(
        jsonEncode([engineRow()]),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final rows = await service().getBookInbox();

    expect(rows, hasLength(1));
    expect(rows.single.inputRawText, 'paid 40 for fuel');
    expect(
      asked.path,
      '/multimodal/inbox',
      reason:
          'a trailing slash is a different address from the one the engine answers',
    );
    expect(asked.queryParameters['view'], 'incomplete');
  });

  test(
    'a capture can be deleted, because the engine confirms rather than empties',
    () async {
      ApiClient.transport = MockClient((request) async {
        expect(request.method, 'DELETE');
        return http.Response(
          jsonEncode({'ok': true}),
          200,
          headers: {'content-type': 'application/json'},
        );
      });

      await expectLater(service().deleteTask('capture-1'), completes);
    },
  );

  test(
    'a correction is sent in the engine\'s names and read back as one',
    () async {
      late Map<String, dynamic> sent;
      ApiClient.transport = MockClient((request) async {
        sent = jsonDecode(request.body) as Map<String, dynamic>;
        return http.Response(
          jsonEncode({
            'id': 'correction-1',
            'task_id': 'capture-1',
            'field_name': 'input_raw_text',
            'original_value': 'paid 40 for fuel',
            'corrected_value': 'paid 45 for fuel',
            'reason': 'read it wrong',
            'created_at': 1763300100.0,
          }),
          200,
          headers: {'content-type': 'application/json'},
        );
      });

      final correction = await service().submitUserCorrection(
        'capture-1',
        UserCorrection(
          taskId: 'capture-1',
          userId: 'self',
          fieldName: 'input_raw_text',
          originalValue: 'paid 40 for fuel',
          correctedValue: 'paid 45 for fuel',
          feedbackType: UserCorrectionType.valueCorrection,
          comment: 'read it wrong',
          submittedAt: DateTime.utc(2026, 11, 16, 9),
        ),
      );

      expect(sent['field_name'], 'input_raw_text');
      expect(sent['corrected_value'], 'paid 45 for fuel');
      expect(sent['reason'], 'read it wrong');
      expect(correction.fieldName, 'input_raw_text');
      expect(correction.correctedValue, 'paid 45 for fuel');
      expect(correction.comment, 'read it wrong');
      expect(correction.feedbackType, UserCorrectionType.valueCorrection);
    },
  );
}
