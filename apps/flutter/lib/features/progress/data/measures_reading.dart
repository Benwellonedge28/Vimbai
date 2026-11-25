// The figures a Book measures itself by, as rows it owns.
//
// ADR-017 made a Book's measures data: each is a row naming what it reads, its unit and whether it leads. So the phone
// asks the Book what it measures rather than carrying a list of figures in code. A business whose trade is selling cars
// and one whose trade is keeping a clinic can lead with different figures, and neither needs a release.
//
// A row that cannot be read comes back with the reason beside it instead of disappearing, so one mistaken measure does not
// hide the rest. This carries that through unchanged: the screen shows the row, its problem, and the others.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One figure the Book measures itself by.
class MeasureRow {
  const MeasureRow({
    required this.name,
    required this.label,
    required this.unit,
    required this.value,
    required this.leads,
    this.problem,
  });

  final String name;

  /// The Book's own words for the figure, not the engine's.
  final String label;

  /// `money`, `percent` or `count`.
  final String unit;

  /// The figure as text, or null when the row could not be read.
  final String? value;

  /// Whether this is a figure the Book leads with.
  final bool leads;

  /// Why the row could not be read, in the Book's words.
  final String? problem;

  bool get isMoney => unit == 'money';
  bool get isPercent => unit == 'percent';

  factory MeasureRow.fromJson(Map<String, dynamic> json) {
    final raw = json['value'];
    return MeasureRow(
      name: (json['name'] as String?) ?? '',
      label: (json['label'] as String?) ?? '',
      unit: (json['unit'] as String?) ?? 'money',
      value: raw is String
          ? raw
          : (raw is num ? raw.toString() : (raw == null ? null : '$raw')),
      leads: json['leads'] == true,
      problem: (json['problem'] as String?)?.trim().isEmpty ?? true
          ? null
          : json['problem'] as String,
    );
  }
}

/// What the Book measures itself by, over a window.
class MeasuresReading {
  const MeasuresReading({
    required this.from,
    required this.to,
    required this.rows,
  });

  final String from;
  final String to;
  final List<MeasureRow> rows;

  /// The figures the Book leads with, in its own order.
  List<MeasureRow> get leading => [
    for (final row in rows)
      if (row.leads) row,
  ];

  bool get isEmpty => rows.isEmpty;

  factory MeasuresReading.fromJson(Map<String, dynamic> json) =>
      MeasuresReading(
        from: (json['from'] as String?) ?? '',
        to: (json['to'] as String?) ?? '',
        rows: [
          for (final row in (json['measures'] as List?) ?? const [])
            if (row is Map<String, dynamic>) MeasureRow.fromJson(row),
        ],
      );
}

class BookMeasuresService extends ChangeNotifier {
  BookMeasuresService.fresh({http.Client? client}) : _client = client;

  static final BookMeasuresService instance = BookMeasuresService.fresh();

  http.Client? _client;

  /// A test replaces the transport, so nothing here needs a server.
  void useClient(http.Client client) => _client = client;

  http.Client get _http => _client ?? http.Client();

  static const String _base = AppConfig.apiUrl;

  String? _token;

  void useToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// The Book's own measures over the window ending on [to].
  Future<MeasuresReading> read({required String to, String? from}) async {
    final query = {
      'to': to,
      if (from != null && from.trim().isNotEmpty) 'from': from.trim(),
    };
    final answer = await _http.get(
      Uri.parse('$_base/accounting/measures').replace(queryParameters: query),
      headers: _headers,
    );
    if (answer.statusCode != 200) throw Exception(_refusalOf(answer));
    return MeasuresReading.fromJson(
      jsonDecode(answer.body) as Map<String, dynamic>,
    );
  }

  String _refusalOf(http.Response answer) {
    try {
      final body = jsonDecode(answer.body);
      if (body is Map<String, dynamic>) {
        for (final key in ['detail', 'message', 'error']) {
          final words = body[key];
          if (words is String && words.trim().isNotEmpty) return words;
        }
      }
    } catch (_) {
      // the body was not the engine's envelope, so the status is all there is to say
    }
    return 'The engine refused this: ${answer.statusCode}.';
  }
}
