// The fixed asset register's doors at the engine.
//
// The register is kept in the engine, one row per thing owned, with what it cost and what is left of that cost. Every
// figure on screen comes back from here: this service records what was bought, charges a month's depreciation and records
// a disposal, and never adds up a value of its own.
//
// Amounts travel as exact decimal text, six places, and are printed through ExactAmount.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/core/api/session_token.dart';
import 'package:vimbai_mobile_client/features/assets/data/fixed_asset_register.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// The asset register doors of one Book.
class FixedAssetsService {
  FixedAssetsService._();

  static final FixedAssetsService instance = FixedAssetsService._();

  /// A second instance for a test that needs a transport of its own.
  FixedAssetsService.fresh();

  static const String _base = AppConfig.apiUrl;
  static const String _door = '/records/fixed-assets-register';

  http.Client _client = http.Client();

  /// A test calls the service against a transport it controls. Nothing in the app sets this.
  @visibleForTesting
  void useClient(http.Client client) => _client = client;

  String? _token;

  /// A test may act as a particular session; the app leaves this alone so the session's own token is used.
  @visibleForTesting
  void setAuthToken(String token) => _token = token;

  Map<String, String> get _headers => {
    'Content-Type': 'application/json',
    if (SessionToken.or(_token) != null)
      'Authorization': 'Bearer ${SessionToken.or(_token)}',
    ...BookContext.instance.headers(),
  };

  /// Everything the Book owns, disposed things included, as the engine lists it.
  Future<AssetRegister> register() async {
    final answer = await _client.get(
      Uri.parse('$_base$_door/assets'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return AssetRegister.fromJson(jsonDecode(answer.body));
  }

  /// One asset, as it stands now rather than as the list remembered it.
  Future<FixedAsset> asset(String id) async {
    final answer = await _client.get(
      Uri.parse('$_base$_door/assets/$id'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return FixedAsset.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Every month's charge recorded against one asset, newest first.
  Future<List<DepreciationCharge>> history(String id) async {
    final answer = await _client.get(
      Uri.parse('$_base$_door/assets/$id/depreciation'),
      headers: _headers,
    );
    if (answer.statusCode != 200) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    final rows = [
      for (final row in (jsonDecode(answer.body) as List?) ?? const [])
        DepreciationCharge.fromJson(row as Map<String, dynamic>),
    ];
    rows.sort((a, b) => b.period.compareTo(a.period));
    return rows;
  }

  /// Record something the business has bought and will use for years.
  ///
  /// Three things are refused here rather than sent, because the engine would faithfully accept them and the register
  /// would carry a row nobody can make sense of later: a cost of nothing, a life of no years, and a salvage value at or
  /// above the cost. A vehicle with no useful life is charged nothing for ever, which reads as an asset that never ages.
  Future<FixedAsset> buy({
    required String code,
    required String name,
    required AssetCategory category,
    required ExactAmount cost,
    required int usefulLifeYears,
    ExactAmount? salvageValue,
    required String acquiredOn,
    DepreciationMethod method = DepreciationMethod.straightLine,
    ExactAmount? rate,
    String location = '',
    String department = '',
  }) async {
    if (code.trim().isEmpty) {
      throw Exception('Give the asset a code, so it can be found later.');
    }
    if (name.trim().isEmpty) {
      throw Exception('Say what was bought.');
    }
    if (cost.isZero) {
      throw Exception('Something bought for nothing is not an asset.');
    }
    if (cost.isNegative) {
      throw Exception('An asset cannot cost less than nothing.');
    }
    if (usefulLifeYears <= 0) {
      throw Exception(
        'Say how many years it will be used. With no life the register '
        'would never write any of it off.',
      );
    }
    final salvage = salvageValue ?? ExactAmount.zero;
    if (salvage.isNegative) {
      throw Exception(
        'What it is worth at the end cannot be less than nothing.',
      );
    }
    if (salvage >= cost) {
      throw Exception(
        'What it is worth at the end (${salvage.toDecimalString()}) has to be '
        'less than what it cost (${cost.toDecimalString()}).',
      );
    }
    if (acquiredOn.trim().isEmpty) {
      throw Exception('Say when it was bought.');
    }
    // a rate is a share of the asset a year: nothing, or more than the whole of it, is a mistake the engine refuses, so
    // it is refused here where the person can see why
    final share = rate ?? ExactAmount.zero;
    if (share.isNegative || share > ExactAmount.parse('100')) {
      throw Exception(
        'The rate has to be a percentage of the asset a year, between 0 and '
        '100.',
      );
    }
    final answer = await _client.post(
      Uri.parse('$_base$_door/assets'),
      headers: _headers,
      body: jsonEncode({
        'asset_code': code.trim(),
        'asset_name': name.trim(),
        'category': category.wire,
        'acquisition_date': acquiredOn.trim(),
        'acquisition_cost': cost.toDecimalString(),
        'useful_life_years': usefulLifeYears,
        'salvage_value': salvage.toDecimalString(),
        'depreciation_method': method.wire,
        if (!share.isZero) 'depreciation_rate': share.toDecimalString(),
        'location': location.trim(),
        'department': department.trim(),
      }),
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    return FixedAsset.fromJson(jsonDecode(answer.body) as Map<String, dynamic>);
  }

  /// Charge one month of depreciation, and answer with the asset as it stands afterwards.
  ///
  /// A month already charged is refused by the engine, which is the check that matters: charging the same month twice
  /// would write the asset down twice and nobody would notice from the register alone.
  Future<FixedAsset> chargeMonth({
    required String id,
    required String period,
  }) async {
    if (!_isMonth(period)) {
      throw Exception('Say which month, as 2026-11.');
    }
    final answer = await _client.post(
      Uri.parse('$_base$_door/assets/$id/depreciate?period=$period'),
      headers: _headers,
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
    // the engine answers with the charge it recorded, so the asset is read back rather than assumed
    return asset(id);
  }

  /// Record that the thing is gone: sold, scrapped, or given away.
  ///
  /// The engine works out the gain or loss from what it was worth and what it fetched, so nothing is worked out here.
  Future<void> dispose({
    required String id,
    required String disposalDate,
    required ExactAmount proceeds,
    required String method,
    String notes = '',
  }) async {
    if (disposalDate.trim().isEmpty) {
      throw Exception('Say when it went.');
    }
    if (proceeds.isNegative) {
      throw Exception('What it fetched cannot be less than nothing.');
    }
    final query = Uri(
      queryParameters: {
        'disposal_date': disposalDate.trim(),
        'disposal_value': proceeds.toDecimalString(),
        'disposal_method': method.trim().isEmpty ? 'sale' : method.trim(),
        if (notes.trim().isNotEmpty) 'notes': notes.trim(),
      },
    ).query;
    final answer = await _client.post(
      Uri.parse('$_base$_door/assets/$id/dispose?$query'),
      headers: _headers,
    );
    if (answer.statusCode != 200 && answer.statusCode != 201) {
      throw Exception(
        'The engine answered ${answer.statusCode}: ${answer.body}',
      );
    }
  }

  static bool _isMonth(String period) {
    final match = RegExp(r'^\d{4}-(0[1-9]|1[0-2])$').firstMatch(period.trim());
    return match != null;
  }
}
