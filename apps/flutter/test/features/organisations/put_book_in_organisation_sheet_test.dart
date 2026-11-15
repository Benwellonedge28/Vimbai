import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/organisations/data/organisation_service.dart';
import 'package:vimbai_mobile_client/features/organisations/models/organisation_models.dart';
import 'package:vimbai_mobile_client/features/organisations/presentation/put_book_in_organisation_sheet.dart';

/// Answers from memory, so the sheet can be walked without a network.
class _FakeService extends OrganisationService {
  _FakeService({this.mineAnswer = const [], this.loadFailure, this.moveFailure})
    : super(token: 'a-token');

  final List<VOrganisation> mineAnswer;
  final String? loadFailure;
  final String? moveFailure;
  final List<String> moved = [];
  final List<String> madePrivate = [];

  @override
  Future<List<VOrganisation>> mine() async {
    if (loadFailure != null) throw OrganisationException(loadFailure!);
    return mineAnswer;
  }

  @override
  Future<void> makePrivate(String organisationId, String bookId) async {
    if (moveFailure != null) throw OrganisationException(moveFailure!);
    madePrivate.add('$organisationId/$bookId');
  }

  @override
  Future<void> moveBook(String organisationId, String bookId) async {
    if (moveFailure != null) throw OrganisationException(moveFailure!);
    moved.add('$organisationId/$bookId');
  }
}

Future<bool?> _open(
  WidgetTester tester,
  OrganisationService service, {
  String? organisationId,
}) async {
  bool? result;
  await tester.pumpWidget(
    MaterialApp(
      home: Builder(
        builder: (context) => Scaffold(
          body: TextButton(
            onPressed: () async {
              result = await PutBookInOrganisationSheet.show(
                context,
                bookId: 'book-7',
                bookName: 'Household',
                organisationId: organisationId,
                service: service,
              );
            },
            child: const Text('open'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.text('open'));
  await tester.pumpAndSettle();
  return result;
}

void main() {
  testWidgets('only organisations the person can put a Book into are offered', (
    tester,
  ) async {
    final service = _FakeService(
      mineAnswer: const [
        VOrganisation(id: 'org-1', name: 'Holdings', role: 'owner'),
        VOrganisation(id: 'org-2', name: 'Stokvel', role: 'viewer'),
      ],
    );
    await _open(tester, service);

    expect(find.text('Holdings'), findsOneWidget);
    expect(find.text('Stokvel'), findsNothing);
  });

  testWidgets('choosing one puts the Book there and reports it done', (
    tester,
  ) async {
    final service = _FakeService(
      mineAnswer: const [
        VOrganisation(id: 'org-1', name: 'Holdings', role: 'owner'),
      ],
    );
    await _open(tester, service);
    await tester.tap(find.text('Holdings'));
    await tester.pumpAndSettle();

    expect(service.moved, ['org-1/book-7']);
  });

  testWidgets('a refusal is shown in the engine words and moves nothing', (
    tester,
  ) async {
    final service = _FakeService(
      mineAnswer: const [
        VOrganisation(id: 'org-1', name: 'Holdings', role: 'owner'),
      ],
      moveFailure: 'You do not hold this Book',
    );
    await _open(tester, service);
    await tester.tap(find.text('Holdings'));
    await tester.pumpAndSettle();

    expect(find.text('You do not hold this Book'), findsWidgets);
    expect(service.moved, isEmpty);
  });

  testWidgets('with no organisation it says so instead of showing nothing', (
    tester,
  ) async {
    await _open(tester, _FakeService());
    expect(
      find.textContaining('no organisation to put it in yet'),
      findsOneWidget,
    );
  });

  testWidgets('a failed read says so and offers nothing to tap', (
    tester,
  ) async {
    final service = _FakeService(loadFailure: 'Could not reach the server');
    await _open(tester, service);

    expect(find.text('Could not reach the server'), findsOneWidget);
    expect(
      find.textContaining('no organisation to put it in yet'),
      findsNothing,
    );
    expect(service.moved, isEmpty);
  });

  testWidgets('not now means not now', (tester) async {
    final service = _FakeService(
      mineAnswer: const [
        VOrganisation(id: 'org-1', name: 'Holdings', role: 'owner'),
      ],
    );
    await _open(tester, service);
    await tester.tap(find.text('Not now'));
    await tester.pumpAndSettle();

    expect(service.moved, isEmpty);
    expect(find.text('Holdings'), findsNothing);
  });

  testWidgets('a Book already shared can be made private again', (
    tester,
  ) async {
    final service = _FakeService(
      mineAnswer: const [
        VOrganisation(id: 'org-1', name: 'Holdings', role: 'owner'),
      ],
    );
    await _open(tester, service, organisationId: 'org-1');

    expect(find.text('Keep this Book private'), findsOneWidget);
    await tester.tap(find.text('Keep this Book private'));
    await tester.pumpAndSettle();

    expect(service.madePrivate, ['org-1/book-7']);
    expect(service.moved, isEmpty);
  });

  testWidgets('a private Book is not offered the choice to become private', (
    tester,
  ) async {
    final service = _FakeService(
      mineAnswer: const [
        VOrganisation(id: 'org-1', name: 'Holdings', role: 'owner'),
      ],
    );
    await _open(tester, service);

    expect(find.text('Keep this Book private'), findsNothing);
    expect(find.text('Holdings'), findsOneWidget);
  });
}
