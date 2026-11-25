// What this Book measures.
//
// What this pins: each measure says what it counts rather than only what it came to, a measure can be dropped, and a new
// one is sent as a row the engine checks (so the words a person types become a name the engine accepts).
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/progress/data/measures_reading.dart';
import 'package:vimbai_mobile_client/features/progress/presentation/what_i_measure_page.dart';

/// Stands in for the engine: it answers with the rows it was given and remembers what was sent.
class _FakeMeasures extends BookMeasuresService {
  _FakeMeasures(this.rows) : super.fresh();

  List<MeasureRow> rows;
  final List<Map<String, dynamic>> added = [];
  final List<String> removed = [];
  String? refusal;

  @override
  Future<MeasuresReading> read({required String to, String? from}) async {
    if (refusal != null) throw Exception(refusal);
    return MeasuresReading(from: '', to: to, rows: rows);
  }

  @override
  Future<void> add(Map<String, dynamic> measure) async {
    if (refusal != null) throw Exception(refusal);
    added.add(measure);
    rows = [
      ...rows,
      MeasureRow(
        name: measure['name'] as String,
        label: measure['label'] as String,
        unit: (measure['unit'] as String?) ?? 'money',
        value: '0',
        leads: measure['leads'] == true,
        reads: Map<String, dynamic>.from(measure)..remove('label'),
      ),
    ];
  }

  @override
  Future<void> remove(String name) async {
    if (refusal != null) throw Exception(refusal);
    removed.add(name);
    rows = [
      for (final row in rows)
        if (row.name != name) row,
    ];
  }
}

MeasureRow _measure({
  String name = 'revenue',
  String label = 'Money in from trading',
  String unit = 'money',
  bool leads = true,
  Map<String, dynamic> reads = const {
    'source': 'accounts',
    'types': ['revenue'],
    'window': 'period',
  },
}) => MeasureRow(
  name: name,
  label: label,
  unit: unit,
  value: '100',
  leads: leads,
  reads: reads,
);

Future<void> _open(WidgetTester tester, _FakeMeasures measures) async {
  tester.view.physicalSize = const Size(1000, 2400);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(home: WhatIMeasurePage(measures: measures)),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('a measure says what it counts, not only what it came to', (
    tester,
  ) async {
    await _open(tester, _FakeMeasures([_measure()]));

    expect(find.text('Money in from trading'), findsOneWidget);
    expect(
      find.textContaining('its revenue accounts moved over the period'),
      findsOneWidget,
    );
  });

  testWidgets('a share says which measures it is worked out from', (
    tester,
  ) async {
    await _open(
      tester,
      _FakeMeasures([
        _measure(
          name: 'margin',
          label: 'Profit as a share of trading',
          unit: 'percent',
          leads: false,
          reads: const {'source': 'percent', 'of': 'profit', 'over': 'revenue'},
        ),
      ]),
    );

    expect(find.text('profit as a share of revenue'), findsOneWidget);
  });

  testWidgets('a measure can be dropped, and says nothing else changes', (
    tester,
  ) async {
    final measures = _FakeMeasures([_measure()]);
    await _open(tester, measures);

    await tester.tap(find.byTooltip('Stop measuring Money in from trading'));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Nothing it counted is changed'),
      findsOneWidget,
    );
    await tester.tap(find.text('Stop measuring it'));
    await tester.pumpAndSettle();

    expect(measures.removed, ['revenue']);
    expect(find.text('Money in from trading'), findsNothing);
  });

  testWidgets('keeping a measure leaves it alone', (tester) async {
    final measures = _FakeMeasures([_measure()]);
    await _open(tester, measures);

    await tester.tap(find.byTooltip('Stop measuring Money in from trading'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Keep it'));
    await tester.pumpAndSettle();

    expect(measures.removed, isEmpty);
    expect(find.text('Money in from trading'), findsOneWidget);
  });

  testWidgets('a new measure is sent as a row the engine can check', (
    tester,
  ) async {
    final measures = _FakeMeasures([_measure()]);
    await _open(tester, measures);

    await tester.tap(find.text('Add a measure'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).first, 'Marketing spend');
    await tester.tap(find.text('Add it'));
    await tester.pumpAndSettle();

    expect(measures.added, hasLength(1));
    expect(measures.added.single['name'], 'marketing_spend');
    expect(measures.added.single['label'], 'Marketing spend');
    expect(measures.added.single['source'], 'accounts');
    expect(measures.added.single['types'], ['expense']);
    expect(measures.added.single['window'], 'period');
  });

  testWidgets('a measure with no name is refused before it is sent', (
    tester,
  ) async {
    final measures = _FakeMeasures([_measure()]);
    await _open(tester, measures);

    await tester.tap(find.text('Add a measure'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Add it'));
    await tester.pumpAndSettle();

    expect(find.text('Say what the measure is called.'), findsOneWidget);
    expect(measures.added, isEmpty);
  });

  testWidgets('the engine\'s refusal is shown as it came', (tester) async {
    final measures = _FakeMeasures([_measure()]);
    await _open(tester, measures);

    measures.refusal =
        'This Book already measures revenue. Give the new one a different name.';
    await tester.tap(find.text('Add a measure'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).first, 'Revenue');
    await tester.tap(find.text('Add it'));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('This Book already measures revenue.'),
      findsOneWidget,
    );
  });

  testWidgets('a Book that measures nothing says how to start', (tester) async {
    await _open(tester, _FakeMeasures(const []));

    expect(find.text('This Book measures nothing yet.'), findsOneWidget);
    expect(find.textContaining('Add a figure you watch'), findsOneWidget);
  });

  test('the doors send the row and the name the engine expects', () async {
    final asked = <String>[];
    final sent = <String>[];
    final service = BookMeasuresService.fresh(
      client: MockClient((request) async {
        asked.add('${request.method} ${request.url.path}');
        sent.add(request.body);
        if (request.method == 'DELETE') {
          return http.Response('{"deleted": "margin"}', 200);
        }
        if (request.method == 'POST') {
          return http.Response('{"name": "marketing_spend"}', 201);
        }
        return http.Response(
          jsonEncode({'from': '', 'to': '2026-11-25', 'measures': []}),
          200,
        );
      }),
    );
    service.useToken('token');

    await service.add({'name': 'marketing_spend', 'label': 'Marketing spend'});
    await service.remove('margin');

    expect(asked, [
      'POST /accounting/measures',
      'DELETE /accounting/measures/margin',
    ]);
    expect(sent.first, contains('marketing_spend'));
  });
}
