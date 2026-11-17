import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/npo/data/npo_scale_service.dart';
import 'package:vimbai_mobile_client/features/npo/presentation/npo_page.dart';

/// A client that records what the page asks for instead of sending it.
class _RecordingService extends NpoScaleService {
  _RecordingService() : super.forTest();

  final List<Map<String, Object>> created = [];
  final List<String> revenues = [];

  @override
  Future<List<NpoOrg>> myOrgs() async => const [];

  @override
  Future<Map<String, dynamic>> createOrg(
    String name, {
    String orgType = 'nonprofit',
    ExactAmount annualRevenue = ExactAmount.zero,
    int headcount = 0,
  }) async {
    created.add({
      'name': name,
      'annual_revenue': annualRevenue.toLedgerString(),
    });
    return const {'org_id': 'org-1'};
  }
}

void main() {
  testWidgets(
    'an organisation revenue larger than a double travels as digits',
    (tester) async {
      final service = _RecordingService();
      await tester.pumpWidget(MaterialApp(home: NpoPage(service: service)));
      await tester.pumpAndSettle();

      await tester.tap(find.text('New organization'));
      await tester.pumpAndSettle();

      const huge = '123456789012345678901234567890.123456';
      final fields = find.descendant(
        of: find.byType(AlertDialog),
        matching: find.byType(TextField),
      );
      await tester.enterText(fields.at(0), 'Harare Trust');
      await tester.enterText(fields.at(1), huge);
      await tester.tap(find.widgetWithText(ElevatedButton, 'Create'));
      await tester.pumpAndSettle();

      expect(service.created, hasLength(1));
      expect(
        service.created.single['annual_revenue'],
        huge,
        reason: 'the figure sets the size band, so it must arrive as typed',
      );
    },
  );

  testWidgets('a revenue that is not a number is refused, not saved as zero', (
    tester,
  ) async {
    final service = _RecordingService();
    await tester.pumpWidget(MaterialApp(home: NpoPage(service: service)));
    await tester.pumpAndSettle();

    await tester.tap(find.text('New organization'));
    await tester.pumpAndSettle();

    final fields = find.descendant(
      of: find.byType(AlertDialog),
      matching: find.byType(TextField),
    );
    await tester.enterText(fields.at(0), 'Harare Trust');
    await tester.enterText(fields.at(1), 'a lot');
    await tester.tap(find.widgetWithText(ElevatedButton, 'Create'));
    await tester.pumpAndSettle();

    expect(find.textContaining('must be a number'), findsOneWidget);
    expect(
      service.created,
      isEmpty,
      reason: 'nothing was created rather than an organisation with no revenue',
    );
  });
}
