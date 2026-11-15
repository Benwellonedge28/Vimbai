// What the organisation page shows in each of its three states.
//
// The distinction that matters is the one a person acts on: "you have none yet" invites them to open one, while a failed call
// must not pretend the same thing - somebody who is told they have no organisations when the server never answered will try to
// open a second one they already have.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/organisations/data/organisation_service.dart';
import 'package:vimbai_mobile_client/features/organisations/models/organisation_models.dart';
import 'package:vimbai_mobile_client/features/organisations/presentation/organisation_page.dart';

/// A service that answers from memory, so the page can be walked without a network.
class _FakeService extends OrganisationService {
  _FakeService({this.existing = const [], this.failure})
    : super(token: 'a-token');

  final List<VOrganisation> existing;
  final String? failure;
  int created = 0;

  @override
  Future<List<VOrganisation>> mine() async {
    if (failure != null) throw OrganisationException(failure!);
    return existing;
  }

  @override
  Future<VOrganisation> create(String name) async {
    created++;
    if (failure != null) throw OrganisationException(failure!);
    return VOrganisation(id: 'org-new', name: name, role: 'owner');
  }
}

Future<void> _open(WidgetTester tester, OrganisationService service) async {
  SharedPreferences.setMockInitialValues({});
  ApiClient.timeout = const Duration(milliseconds: 200);
  await tester.pumpWidget(
    MaterialApp(home: OrganisationPage(service: service)),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('with none yet, the page invites the person to open one', (
    tester,
  ) async {
    await _open(tester, _FakeService());

    expect(find.text('You have not set one up yet.'), findsOneWidget);
    expect(find.text('Open an organisation'), findsOneWidget);
  });

  testWidgets('an organisation already held is listed with the role', (
    tester,
  ) async {
    await _open(
      tester,
      _FakeService(
        existing: const [
          VOrganisation(id: 'org-1', name: 'Mukandara Holdings', role: 'owner'),
        ],
      ),
    );

    expect(find.text('Mukandara Holdings'), findsOneWidget);
    expect(find.text('You are its owner'), findsOneWidget);
    expect(find.text('You have not set one up yet.'), findsNothing);
  });

  testWidgets('a failed call is reported as a failure, never as none', (
    tester,
  ) async {
    await _open(
      tester,
      _FakeService(failure: 'Could not read your organisations'),
    );

    expect(find.text('Could not read your organisations'), findsOneWidget);
    expect(
      find.text('You have not set one up yet.'),
      findsNothing,
      reason: 'a server that did not answer proves nothing about what exists',
    );
  });

  testWidgets('opening one puts it on the page', (tester) async {
    final service = _FakeService();
    await _open(tester, service);

    await tester.enterText(find.byType(TextField), 'Mukandara Holdings');
    await tester.tap(find.text('Open an organisation'));
    await tester.pumpAndSettle();

    expect(service.created, 1);
    expect(find.text('Mukandara Holdings'), findsOneWidget);
    expect(find.text('You are its owner'), findsOneWidget);
  });

  testWidgets('a nameless organisation is not sent', (tester) async {
    final service = _FakeService();
    await _open(tester, service);

    await tester.tap(find.text('Open an organisation'));
    await tester.pumpAndSettle();

    expect(service.created, 0);
    expect(find.text('Give the organisation a name.'), findsOneWidget);
  });
}
