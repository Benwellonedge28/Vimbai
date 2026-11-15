// What the organisation routes have to do for the app, proven against a transport rather than a server.
//
// These are the three calls a person's first run depends on: see what you belong to, open one, and put a Book inside it. The
// point of testing them here is that a refusal from the engine keeps its own words, and an empty list is not confused with a
// failed call - a page that shows "you have none yet" when the request failed would tell a lie the person then acts on.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/organisations/data/organisation_service.dart';

void main() {
  final originalTransport = ApiClient.transport;

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test('an organisation the engine sends reads as an organisation', () async {
    late Uri asked;
    ApiClient.transport = MockClient((request) async {
      asked = request.url;
      return http.Response(
        jsonEncode({
          'count': 1,
          'organizations': [
            {
              'id': 'org-1',
              'name': 'Mukandara Holdings',
              'slug': 'mukandara-holdings',
              'role': 'owner',
              'book_count': 2,
            },
          ],
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final mine = await OrganisationService(token: 'a-token').mine();

    expect(mine, hasLength(1));
    expect(mine.first.name, 'Mukandara Holdings');
    expect(mine.first.role, 'owner');
    expect(mine.first.canManageBooks, isTrue);
    // the route the app calls is the identity door, because that is where the engine answers it
    expect(asked.path, '/identity/organizations');
  });

  test('no organisations yet is an empty list, not a failure', () async {
    ApiClient.transport = MockClient(
      (_) async => http.Response(
        jsonEncode({'count': 0, 'organizations': []}),
        200,
        headers: {'content-type': 'application/json'},
      ),
    );

    expect(await OrganisationService(token: 'a-token').mine(), isEmpty);
  });

  test(
    'a failed call is said to have failed rather than answered as empty',
    () async {
      ApiClient.transport = MockClient(
        (_) async => http.Response(
          jsonEncode({'detail': 'Not authenticated'}),
          401,
          headers: {'content-type': 'application/json'},
        ),
      );

      await expectLater(
        OrganisationService(token: 'stale').mine(),
        throwsA(
          isA<OrganisationException>().having((e) => e.status, 'status', 401),
        ),
      );
    },
  );

  test('opening one sends the name and reads back the membership', () async {
    late Map<String, dynamic> sent;
    ApiClient.transport = MockClient((request) async {
      sent = jsonDecode(request.body) as Map<String, dynamic>;
      return http.Response(
        jsonEncode({
          'id': 'org-9',
          'name': 'Mukandara Holdings',
          'slug': 'mukandara-holdings',
          'role': 'owner',
        }),
        201,
        headers: {'content-type': 'application/json'},
      );
    });

    final made = await OrganisationService(
      token: 'a-token',
    ).create('  Mukandara Holdings  ');

    expect(sent['name'], 'Mukandara Holdings');
    expect(made.id, 'org-9');
    expect(made.canManageBooks, isTrue);
  });

  test(
    'a nameless organisation is refused before it reaches the wire',
    () async {
      var called = false;
      ApiClient.transport = MockClient((_) async {
        called = true;
        return http.Response('{}', 201);
      });

      await expectLater(
        OrganisationService(token: 'a-token').create('   '),
        throwsA(isA<OrganisationException>()),
      );
      expect(called, isFalse, reason: 'nothing to name means nothing to send');
    },
  );

  test('a refusal from the engine keeps its own words', () async {
    ApiClient.transport = MockClient(
      (_) async => http.Response(
        jsonEncode({'detail': 'Name already taken'}),
        400,
        headers: {'content-type': 'application/json'},
      ),
    );

    await expectLater(
      OrganisationService(token: 'a-token').create('Holdings'),
      throwsA(
        isA<OrganisationException>().having(
          (e) => e.message,
          'message',
          'Name already taken',
        ),
      ),
    );
  });

  test(
    'putting a Book in an organisation calls the route that needs both halves',
    () async {
      late http.Request seen;
      ApiClient.transport = MockClient((request) async {
        seen = request;
        return http.Response(jsonEncode({'organisation_id': 'org-1'}), 200);
      });

      await OrganisationService(token: 'a-token').moveBook('org-1', 'book-7');

      expect(seen.method, 'PUT');
      expect(seen.url.path, '/identity/organizations/org-1/books/book-7');
    },
  );

  test(
    'a member who cannot move a Book is told so, not silently ignored',
    () async {
      ApiClient.transport = MockClient(
        (_) async => http.Response(
          jsonEncode({'detail': 'Forbidden'}),
          403,
          headers: {'content-type': 'application/json'},
        ),
      );

      await expectLater(
        OrganisationService(token: 'a-token').moveBook('org-1', 'book-7'),
        throwsA(
          isA<OrganisationException>().having((e) => e.status, 'status', 403),
        ),
      );
    },
  );

  test('a Book is found in the organisation that holds it', () async {
    ApiClient.transport = MockClient((request) async {
      final path = request.url.path;
      if (path.endsWith('/identity/organizations')) {
        return http.Response(
          jsonEncode({
            'organizations': [
              {'id': 'org-1', 'name': 'Holdings', 'role': 'owner'},
              {'id': 'org-2', 'name': 'Stokvel', 'role': 'owner'},
            ],
          }),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      final inOrg1 = path.endsWith('/org-1/books');
      return http.Response(
        jsonEncode({
          'books': inOrg1
              ? [
                  {'id': 'book-7'},
                ]
              : [
                  {'id': 'book-9'},
                ],
        }),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    final organisation = await OrganisationService(
      token: 'a-token',
    ).ofBook('book-7');

    expect(organisation?.name, 'Holdings');
  });

  test('a Book in nobody\'s organisation is null, not a guess', () async {
    ApiClient.transport = MockClient((request) async {
      final path = request.url.path;
      if (path.endsWith('/identity/organizations')) {
        return http.Response(
          jsonEncode({
            'organizations': [
              {'id': 'org-1', 'name': 'Holdings', 'role': 'owner'},
            ],
          }),
          200,
          headers: {'content-type': 'application/json'},
        );
      }
      return http.Response(
        jsonEncode({'books': <dynamic>[]}),
        200,
        headers: {'content-type': 'application/json'},
      );
    });

    expect(
      await OrganisationService(token: 'a-token').ofBook('book-7'),
      isNull,
    );
  });

  test('a failed read is a failure, never "not shared"', () async {
    ApiClient.transport = MockClient(
      (request) async => http.Response('nope', 503),
    );

    expect(
      () => OrganisationService(token: 'a-token').ofBook('book-7'),
      throwsA(isA<OrganisationException>()),
    );
  });
}
