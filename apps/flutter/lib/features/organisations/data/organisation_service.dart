// Organisations, from the app's side.
//
// The engine has answered these routes for a while (`/identity/organizations`), but nothing in the app could reach them, so a
// person who registered on a phone had an account and a Book and no organisation - and a Book with no organisation cannot be
// shared with anybody, which is most of the point of having one.
//
// Two rules shape this file. Every call goes through [ApiClient], so being offline is reported as being offline rather than as
// an unhandled error. And nothing here invents an organisation in local storage: an organisation is a fact about the server,
// so a failed call leaves the app with no organisation and says so, instead of a local row that will disagree with the engine
// the next time they are compared.

import 'dart:convert';

import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/organisations/models/organisation_models.dart';

class OrganisationService {
  OrganisationService({ApiClient? client, String? token})
    : _client = client ?? ApiClient(),
      _token = token;

  final ApiClient _client;
  String? _token;

  /// The engine reads the caller from the gateway's injected header, so the app only has to say who it is.
  void useToken(String? token) => _token = token;

  Map<String, String> get _headers => {
    'content-type': 'application/json',
    if (_token != null) 'authorization': 'Bearer $_token',
  };

  /// The organisation routes sit behind the identity door, one of the two the gateway leaves open because signing in cannot
  /// require a token. Written as a literal so the build's address scan can see the prefix the app calls.
  Uri get _organisations =>
      Uri.parse('${AppConfig.apiUrl}/identity/organizations');

  /// The organisations this person belongs to, newest answers from the engine included.
  ///
  /// An empty list and a failed call are deliberately different: no organisations means the person has not made one yet,
  /// while a failure means nobody knows. The caller shows an offer in the first case and an honest refusal in the second.
  Future<List<VOrganisation>> mine() async {
    final response = await _client.get(_organisations, headers: _headers);
    if (response.statusCode != 200) {
      throw OrganisationException(
        'Could not read your organisations',
        status: response.statusCode,
      );
    }
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    final rows = (body['organizations'] as List<dynamic>? ?? const []);
    return rows
        .whereType<Map<String, dynamic>>()
        .map(VOrganisation.fromJson)
        .toList();
  }

  /// Open an organisation and become its owner.
  ///
  /// The engine decides the slug from the name and makes the caller its owner, which is the membership every later question
  /// reads. Nothing about that is repeated here: this only sends the name.
  Future<VOrganisation> create(String name) async {
    final trimmed = name.trim();
    if (trimmed.isEmpty) {
      throw OrganisationException('Give the organisation a name');
    }
    final response = await _client.post(
      _organisations,
      headers: _headers,
      body: jsonEncode({'name': trimmed}),
    );
    if (response.statusCode != 201) {
      throw OrganisationException(
        _reason(response.body, 'Could not open the organisation'),
        status: response.statusCode,
      );
    }
    return VOrganisation.fromJson(
      jsonDecode(response.body) as Map<String, dynamic>,
    );
  }

  /// Put a Book inside an organisation.
  ///
  /// The engine asks for both halves - the caller must administer the organisation and hold the Book - so a refusal here is
  /// a real answer about permissions, not a bug to retry around.
  Future<void> moveBook(String organisationId, String bookId) async {
    final response = await _client.put(
      Uri.parse(
        '${AppConfig.apiUrl}/identity/organizations/$organisationId/books/$bookId',
      ),
      headers: _headers,
    );
    if (response.statusCode != 200) {
      throw OrganisationException(
        _reason(response.body, 'Could not move the Book into the organisation'),
        status: response.statusCode,
      );
    }
  }

  /// Take a Book back out of an organisation, leaving it with only the person who holds it.
  ///
  /// The counterpart of [moveBook]: an offer a person can only accept is not an offer. It keeps the engine's own refusal
  /// wording, so a member who is not allowed to do this is told that rather than shown a status number.
  Future<void> makePrivate(String organisationId, String bookId) async {
    final response = await _client.delete(
      Uri.parse(
        '${AppConfig.apiUrl}/identity/organizations/$organisationId/books/$bookId',
      ),
      headers: _headers,
    );
    if (response.statusCode != 200) {
      throw OrganisationException(
        _reason(response.body, 'Could not make the Book private again'),
        status: response.statusCode,
      );
    }
  }

  /// The Books inside an organisation, as the engine lists them.
  Future<List<String>> books(String organisationId) async {
    final response = await _client.get(
      Uri.parse(
        '${AppConfig.apiUrl}/identity/organizations/$organisationId/books',
      ),
      headers: _headers,
    );
    if (response.statusCode != 200) {
      throw OrganisationException(
        'Could not read the organisation\'s Books',
        status: response.statusCode,
      );
    }
    final body = jsonDecode(response.body) as Map<String, dynamic>;
    final rows = (body['books'] as List<dynamic>? ?? const []);
    return rows
        .whereType<Map<String, dynamic>>()
        .map((book) => book['id'] as String? ?? '')
        .where((id) => id.isNotEmpty)
        .toList();
  }

  /// The organisation this person has put [bookId] into, if any.
  ///
  /// Answered by asking the organisations the person can manage which Books they hold, because the engine has no route from a
  /// Book to its organisation; adding one would be a second place that knows the answer. Null means "not in any organisation I
  /// can manage", and an exception means the question could not be asked, which a caller must not report as "not shared".
  Future<VOrganisation?> ofBook(String bookId) async {
    final mine = await this.mine();
    for (final organisation in mine.where((o) => o.canManageBooks)) {
      if ((await books(organisation.id)).contains(bookId)) return organisation;
    }
    return null;
  }

  /// The engine's own words when it gave a reason, so the person is told what happened rather than a status number.
  static String _reason(String body, String fallback) {
    try {
      final decoded = jsonDecode(body);
      if (decoded is Map<String, dynamic>) {
        final said =
            decoded['detail'] ?? decoded['error'] ?? decoded['message'];
        if (said is String && said.trim().isNotEmpty) return said;
      }
      if (decoded is String && decoded.trim().isNotEmpty) return decoded;
    } on FormatException {
      // not JSON: the fallback is the honest answer
    }
    return fallback;
  }
}

/// A refusal from the organisation routes, carrying the status so a caller can tell "not yours" from "not there".
class OrganisationException implements Exception {
  OrganisationException(this.message, {this.status});

  final String message;
  final int? status;

  @override
  String toString() => message;
}
