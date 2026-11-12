// Family & community group (savings club / mukando) client.
//
// Groups, members, per-cycle contributions and payout schedules. The
// API gateway injects X-User-ID from the JWT and X-Book-ID from the
// active Book context, so every group is scoped to the Book.
import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/config.dart';
import 'package:vimbai_mobile_client/services/book_context.dart';

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
        if (_token != null) 'Authorization': 'Bearer $_token',
        ...BookContext.instance.headers(),
      };

  final http.Client _client = http.Client();

  dynamic _decode(http.Response r) {
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    return jsonDecode(r.body);
  }

  Map<String, dynamic> _decodeMap(http.Response r) =>
      _decode(r) as Map<String, dynamic>;

  List<Map<String, dynamic>> _decodeList(http.Response r) =>
      (_decode(r) as List).cast<Map<String, dynamic>>();

  // -- groups ----------------------------------------------------------

  Future<List<Map<String, dynamic>>> listGroups() async =>
      _decodeList(await _client.get(_u('/groups'), headers: _headers));

  Future<Map<String, dynamic>> createGroup({
    required String name,
    String description = '',
    String contributionFrequency = 'monthly',
    double contributionAmount = 0.0,
  }) async {
    // The backend takes scalar POST params as query parameters.
    final r = await _client.post(
      _u('/groups').replace(queryParameters: {
        'name': name,
        'description': description,
        'contribution_frequency': contributionFrequency,
        'contribution_amount': contributionAmount.toString(),
      }),
      headers: _headers,
    );
    return _decodeMap(r);
  }

  Future<Map<String, dynamic>> getGroup(String groupId) async =>
      _decodeMap(await _client.get(_u('/groups/$groupId'), headers: _headers));

  Uri _u(String p) => Uri.parse('$_kBaseUrl$p');

  // -- members ---------------------------------------------------------

  Future<List<Map<String, dynamic>>> listMembers(String groupId) async =>
      _decodeList(await _client.get(
        _u('/groups/$groupId/members'),
        headers: _headers,
      ));

  Future<Map<String, dynamic>> addMember(
    String groupId, {
    required String name,
    String email = '',
    String phone = '',
    double contributionAmount = 0.0,
  }) async {
    final r = await _client.post(
      _u('/groups/$groupId/members').replace(queryParameters: {
        'name': name,
        'email': email,
        'phone': phone,
        'contribution_amount': contributionAmount.toString(),
      }),
      headers: _headers,
    );
    return _decodeMap(r);
  }

  // -- contributions ----------------------------------------------------

  Future<Map<String, dynamic>> contribute(
    String groupId, {
    required String memberId,
    required double amount,
    String notes = '',
  }) async {
    final r = await _client.post(
      _u('/groups/$groupId/contribute').replace(queryParameters: {
        'member_id': memberId,
        'amount': amount.toString(),
        'notes': notes,
      }),
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

  Future<Map<String, dynamic>> advanceCycle(String groupId) async =>
      _decodeMap(await _client.post(
        _u('/groups/$groupId/advance-cycle'),
        headers: _headers,
      ));

  Future<List<Map<String, dynamic>>> listPayouts(String groupId) async =>
      _decodeList(await _client.get(
        _u('/groups/$groupId/payouts'),
        headers: _headers,
      ));
}
