// The group savings page reads its lists out of the names the engine wraps them in.
//
// `groups`, `members`, `contributions` and `payouts` each arrive under their own name, and the service used to cast
// the whole reply to an array, so the first read threw. Both shapes are held here.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/features/group_savings/data/group_savings_service.dart';

void main() {
  test('groups read from the name the engine wraps them in', () async {
    GroupSavingsService.instance.useClient(
      MockClient((request) async {
        return http.Response(
          jsonEncode({
            'groups': [
              {'id': 'g1', 'name': 'Kitchen Club'},
            ],
            'count': 1,
          }),
          200,
        );
      }),
    );

    final groups = await GroupSavingsService.instance.listGroups();

    expect(groups.single['name'], 'Kitchen Club');
  });

  test('a bare array is still read as one', () async {
    GroupSavingsService.instance.useClient(
      MockClient((request) async {
        return http.Response(
          jsonEncode([
            {'id': 'g1', 'name': 'Kitchen Club'},
          ]),
          200,
        );
      }),
    );

    final groups = await GroupSavingsService.instance.listGroups();

    expect(groups, hasLength(1));
  });

  test('an empty wrapped answer is an empty list, not a throw', () async {
    GroupSavingsService.instance.useClient(
      MockClient((request) async {
        return http.Response(jsonEncode({'groups': [], 'count': 0}), 200);
      }),
    );

    expect(await GroupSavingsService.instance.listGroups(), isEmpty);
  });
}
