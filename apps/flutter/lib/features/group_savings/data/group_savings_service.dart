// Family & community group (savings club / mukando) client.
//
// Groups, members, per-cycle contributions and payout schedules. The
// API gateway injects X-User-ID from the JWT and X-Book-ID from the
// active Book context, so every group is scoped to the Book.
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'dart:convert';
import 'package:vimbai_mobile_client/core/api/amount.dart';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

class GroupSavingsService {
  GroupSavingsService._();
  static final GroupSavingsService instance = GroupSavingsService._();

  static const String _kBaseUrl = String.fromEnvironment(
    'VIMBAI_GROUP_SAVINGS_URL',
    defaultValue: '${AppConfig.apiUrl}/family-community-group',
  );

  String? _token;
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  http.Client _client = http.Client();

  /// A test calls the service against a transport it controls, so what the screen reads back from a reply
  /// is proven without a server. The real client is untouched until a test swaps it.
  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  dynamic _decode(http.Response r) {
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    return jsonDecode(r.body);
  }

  Map<String, dynamic> _decodeMap(http.Response r) =>
      _decode(r) as Map<String, dynamic>;

  /// The engine wraps each of these lists under its own name - `groups`, `members`,
  /// `contributions`, `payouts` - so casting the reply straight to a List threw before the
  /// page could show a row. A bare array is still read as one.
  List<Map<String, dynamic>> _decodeList(http.Response r) {
    final decoded = _decode(r);
    final list = decoded is List
        ? decoded
        : (decoded as Map<String, dynamic>).values.firstWhere(
            (value) => value is List,
            orElse: () => const <dynamic>[],
          );
    return (list as List).cast<Map<String, dynamic>>();
  }

  // -- groups ----------------------------------------------------------

  Future<List<Map<String, dynamic>>> listGroups() async =>
      _decodeList(await _client.get(_u('/groups'), headers: _headers));

  Future<Map<String, dynamic>> createGroup({
    required String name,
    String description = '',
    String contributionFrequency = 'monthly',
    ExactAmount contributionAmount = ExactAmount.zero,
  }) async {
    // The backend takes scalar POST params as query parameters.
    final r = await _client.post(
      _u('/groups').replace(
        queryParameters: {
          'name': name,
          'description': description,
          'contribution_frequency': contributionFrequency,
          // A contribution past fifteen digits sent as a double arrives as "1e+30", which is not a figure at all.
          'contribution_amount': contributionAmount.toLedgerString(),
        },
      ),
      headers: _headers,
    );
    return _decodeMap(r);
  }

  Future<Map<String, dynamic>> getGroup(String groupId) async =>
      _decodeMap(await _client.get(_u('/groups/$groupId'), headers: _headers));

  Uri _u(String p) => Uri.parse('$_kBaseUrl$p');

  // -- members ---------------------------------------------------------

  Future<List<Map<String, dynamic>>> listMembers(String groupId) async =>
      _decodeList(
        await _client.get(_u('/groups/$groupId/members'), headers: _headers),
      );

  Future<Map<String, dynamic>> addMember(
    String groupId, {
    required String name,
    String email = '',
    String phone = '',
    ExactAmount contributionAmount = ExactAmount.zero,
  }) async {
    final r = await _client.post(
      _u('/groups/$groupId/members').replace(
        queryParameters: {
          'name': name,
          'email': email,
          'phone': phone,
          // A contribution past fifteen digits sent as a double arrives as "1e+30", which is not a figure at all.
          'contribution_amount': contributionAmount.toLedgerString(),
        },
      ),
      headers: _headers,
    );
    return _decodeMap(r);
  }

  // -- contributions ----------------------------------------------------

  Future<Map<String, dynamic>> contribute(
    String groupId, {
    required String memberId,
    required ExactAmount amount,
    String notes = '',
  }) async {
    final r = await _client.post(
      _u('/groups/$groupId/contribute').replace(
        queryParameters: {
          'member_id': memberId,
          'amount': amount.toLedgerString(),
          'notes': notes,
        },
      ),
      headers: _headers,
    );
    return _decodeMap(r);
  }

  Future<List<Map<String, dynamic>>> listContributions(
    String groupId, {
    int? cycle,
  }) async {
    var url = _u('/groups/$groupId/contributions');
    if (cycle != null) {
      url = url.replace(queryParameters: {'cycle': cycle.toString()});
    }
    return _decodeList(await _client.get(url, headers: _headers));
  }

  // -- cycles & payouts -------------------------------------------------

  Future<Map<String, dynamic>> advanceCycle(String groupId) async => _decodeMap(
    await _client.post(_u('/groups/$groupId/advance-cycle'), headers: _headers),
  );

  Future<List<Map<String, dynamic>>> listPayouts(String groupId) async =>
      _decodeList(
        await _client.get(_u('/groups/$groupId/payouts'), headers: _headers),
      );
}
