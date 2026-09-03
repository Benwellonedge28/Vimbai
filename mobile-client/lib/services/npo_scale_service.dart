// NpoScaleService - mobile client for Vimbai's non-profit platform.
//
// Handles organizations of every size: a community trust is a small
// org with a single branch; a national federation is extra-large with
// chapters and consolidated reporting. The size band is computed on
// the server from revenue, headcount and branch count, and unlocks
// features progressively (donor CRM -> budgets -> branches ->
// dual approval -> federation consolidation).

import 'dart:convert';

import 'package:http/http.dart' as http;

class NpoOrg {
  final String id;
  final String name;
  final String orgType; // nonprofit | commercial
  final String sizeBand;
  final double annualRevenue;
  final int headcount;

  NpoOrg({
    required this.id,
    required this.name,
    required this.orgType,
    required this.sizeBand,
    this.annualRevenue = 0,
    this.headcount = 0,
  });

  factory NpoOrg.fromJson(Map<String, dynamic> j) {
    return NpoOrg(
      id: j['id'] as String,
      name: j['name'] as String? ?? '',
      orgType: j['org_type'] as String? ?? 'nonprofit',
      sizeBand: j['size_band'] as String? ?? 'small',
      annualRevenue: (j['annual_revenue'] as num?)?.toDouble() ?? 0,
      headcount: (j['headcount'] as num?)?.toInt() ?? 0,
    );
  }
}

class NpoScaleService {
  NpoScaleService._();
  static final NpoScaleService instance = NpoScaleService._();

  static const String _kBaseUrl = String.fromEnvironment(
    'VIMBAI_NPO_SCALE_URL',
    defaultValue: 'http://10.0.2.2:8080/npo-scale',
  );

  String? _token;
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
        'Content-Type': 'application/json',
        if (_token != null) 'Authorization': 'Bearer $_token',
      };

  http.Client _client = http.Client();

  Uri _u(String p) => Uri.parse('$_kBaseUrl$p');

  T _decode<T>(http.Response r) {
    if (r.statusCode >= 400) {
      throw Exception('API ${r.statusCode}: ${r.body}');
    }
    return jsonDecode(r.body) as T;
  }

  Future<List<NpoOrg>> myOrgs() async {
    final r = await _client.get(_u('/orgs'), headers: _headers);
    final j = _decode<Map<String, dynamic>>(r);
    return (j['orgs'] as List<dynamic>)
        .map((o) => NpoOrg.fromJson(o as Map<String, dynamic>))
        .toList();
  }

  Future<Map<String, dynamic>> createOrg(
    String name, {
    String orgType = 'nonprofit',
    double annualRevenue = 0,
    int headcount = 0,
  }) async {
    final r = await _client.post(
      _u('/orgs'),
      headers: _headers,
      body: jsonEncode({
        'name': name,
        'org_type': orgType,
        'annual_revenue': annualRevenue,
        'headcount': headcount,
      }),
    );
    return _decode<Map<String, dynamic>>(r);
  }

  Future<Map<String, dynamic>> orgFeatures(String orgId) async {
    final r = await _client.get(
      _u('/orgs/$orgId/features'),
      headers: _headers,
    );
    return _decode<Map<String, dynamic>>(r);
  }

  Future<String> addDonor(String orgId, String name, String type) async {
    final r = await _client.post(
      _u('/orgs/$orgId/donors'),
      headers: _headers,
      body: jsonEncode({'name': name, 'type': type}),
    );
    return (_decode<Map<String, dynamic>>(r))['donor_id'] as String;
  }

  Future<Map<String, dynamic>> addDonation(
    String orgId,
    String donorId,
    double amount, {
    String designation = 'general',
  }) async {
    final r = await _client.post(
      _u('/orgs/$orgId/donations'),
      headers: _headers,
      body: jsonEncode({
        'donor_id': donorId,
        'amount': amount,
        'designation': designation,
      }),
    );
    return _decode<Map<String, dynamic>>(r);
  }

  Future<Map<String, dynamic>> positionReport(String orgId) async {
    final r = await _client.get(
      _u('/orgs/$orgId/reports/position'),
      headers: _headers,
    );
    return _decode<Map<String, dynamic>>(r);
  }

  Future<Map<String, dynamic>> activitiesReport(String orgId) async {
    final r = await _client.get(
      _u('/orgs/$orgId/reports/activities'),
      headers: _headers,
    );
    return _decode<Map<String, dynamic>>(r);
  }

  /// Record business revenue (sale / service) with an automatic receipt.
  Future<Map<String, dynamic>> addRevenue(
    String orgId,
    double amount, {
    String source = 'sale',
    String customer = '',
  }) async {
    final r = await _client.post(
      _u('/orgs/$orgId/revenues'),
      headers: _headers,
      body: jsonEncode({
        'amount': amount,
        'source': source,
        'customer': customer,
      }),
    );
    return _decode<Map<String, dynamic>>(r);
  }

  /// Vendors, purchases and creditors (all org types).
  Future<String> addVendor(String orgId, String name) async {
    final r = await _client.post(
      _u('/orgs/$orgId/vendors'),
      headers: _headers,
      body: jsonEncode({'name': name}),
    );
    return (_decode<Map<String, dynamic>>(r))['vendor_id'] as String;
  }

  Future<String> recordPurchase(
    String orgId,
    String vendorId,
    String description,
    double amount,
  ) async {
    final r = await _client.post(
      _u('/orgs/$orgId/purchases'),
      headers: _headers,
      body: jsonEncode({
        'vendor_id': vendorId,
        'description': description,
        'amount': amount,
      }),
    );
    return (_decode<Map<String, dynamic>>(r))['purchase_id'] as String;
  }

  Future<Map<String, dynamic>> payPurchase(String orgId, String purchaseId) async {
    final r = await _client.post(
      _u('/orgs/$orgId/purchases/$purchaseId/pay'),
      headers: _headers,
    );
    return _decode<Map<String, dynamic>>(r);
  }

  Future<Map<String, dynamic>> creditorsReport(String orgId) async {
    final r = await _client.get(
      _u('/orgs/$orgId/reports/creditors'),
      headers: _headers,
    );
    return _decode<Map<String, dynamic>>(r);
  }

  /// Public receipt verification - lets a donor confirm a receipt.
  Future<bool> verifyReceipt(String token) async {
    final r = await _client.get(_u('/receipts/verify/$token'));
    if (r.statusCode != 200) return false;
    return (jsonDecode(r.body) as Map<String, dynamic>)['valid'] == true;
  }
}
