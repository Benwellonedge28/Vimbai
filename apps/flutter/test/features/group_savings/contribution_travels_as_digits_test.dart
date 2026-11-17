// A group contribution leaves the phone as its own digits. Sent as a double, a large contribution arrived as
// "1e+30", which the engine refused, so a group with a big kitty could not record what it collected.
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:vimbai_mobile_client/core/api/amount.dart';
import 'package:vimbai_mobile_client/features/group_savings/data/group_savings_service.dart';

void main() {
  test('a thirty-digit contribution is sent as plain digits', () async {
    late Uri asked;
    GroupSavingsService.instance.useClient(
      MockClient((request) async {
        asked = request.url;
        return http.Response('{}', 200);
      }),
    );

    const amount = '123456789012345678901234567890.500000';
    await GroupSavingsService.instance.contribute(
      'g1',
      memberId: 'm1',
      amount: ExactAmount.parse(amount),
    );

    expect(
      asked.queryParameters['amount'],
      amount,
      reason: 'a double would have answered 1.2345678901234568e+29',
    );
  });

  test('a group subscription keeps every digit too', () async {
    late Uri asked;
    GroupSavingsService.instance.useClient(
      MockClient((request) async {
        asked = request.url;
        return http.Response('{}', 200);
      }),
    );

    await GroupSavingsService.instance.createGroup(
      name: 'Burial society',
      contributionAmount: ExactAmount.parse('25.000001'),
    );

    expect(asked.queryParameters['contribution_amount'], '25.000001');
  });
}
