// A savings group list that could not be read is not a list of no groups.
//
// The page already kept loading, failure and empty apart, but it printed "Could not load groups: Exception: ..." - the
// engine's sentence buried behind the wrapper - and called its retry "Retry" while every other screen says "Try again".
// The group's own page answered a failed read with the bare exception. These tests hold the reader's side of both.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/group_savings/data/group_savings_service.dart';
import 'package:vimbai_mobile_client/features/group_savings/presentation/group_savings_page.dart';

Widget _page() => const MaterialApp(home: GroupSavingsPage());

void main() {
  testWidgets(
    'a failed read names what could not be read and repeats the engine reason',
    (tester) async {
      GroupSavingsService.instance.useClient(
        MockClient((_) async => http.Response('{}', 503)),
      );

      await tester.pumpWidget(_page());
      await tester.pumpAndSettle();

      expect(
        find.text('Your savings groups could not be read just now.'),
        findsOneWidget,
      );
      expect(find.text('Try again'), findsOneWidget);
      expect(
        find.textContaining('Exception:'),
        findsNothing,
        reason:
            'the wrapper an exception carries is not part of the sentence a person reads',
      );
      expect(
        find.textContaining('No savings groups yet'),
        findsNothing,
        reason:
            'nothing was read, so the page must not claim there are no groups',
      );
    },
  );

  testWidgets('a read that answered with nothing says so plainly', (
    tester,
  ) async {
    GroupSavingsService.instance.useClient(
      MockClient(
        (_) async => http.Response(
          jsonEncode({'total': 0, 'groups': []}),
          200,
          headers: {'content-type': 'application/json'},
        ),
      ),
    );

    await tester.pumpWidget(_page());
    await tester.pumpAndSettle();

    expect(find.textContaining('No savings groups yet'), findsOneWidget);
    expect(
      find.text('Your savings groups could not be read just now.'),
      findsNothing,
    );
  });
}
