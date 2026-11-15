// What a machine serves, read from its own health door.

import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/core/api/machine_profile.dart';

void main() {
  test('a core machine reports the prefixes it serves, by name', () {
    final body = jsonDecode(
      '{"status":"healthy","profile":"core","services_loaded":4,'
      '"services":["accounting","departmental-accounting","audit-compliance","book-sync"]}',
    );
    final profile = MachineProfile.fromHealth(body);
    expect(profile.profile, 'core');
    expect(profile.serves('accounting'), isTrue);
    expect(
      profile.serves('/book-sync'),
      isTrue,
      reason: 'the app may name a prefix either way',
    );
    expect(profile.serves('banking-integration'), isFalse);
    expect(profile.serves('cash-flow-statement'), isFalse);
  });

  test('a full machine serves everything, whether or not it lists them', () {
    final profile = MachineProfile.fromHealth({
      'profile': 'full',
      'services': <String>[],
    });
    expect(profile.serves('banking-integration'), isTrue);
    expect(profile.serves('anything-at-all'), isTrue);
  });

  test('a machine that cannot be asked serves everything', () {
    // Hiding a working feature from a caller of an engine that never answered health is worse than offering one a machine
    // refuses, because the refusal is at least a sentence somebody can read.
    for (final body in [
      null,
      'not json',
      <String, dynamic>{},
      {'profile': 7},
    ]) {
      expect(
        MachineProfile.fromHealth(body).serves('banking-integration'),
        isTrue,
      );
    }
  });

  test(
    'the features a machine will refuse are named in the words a person reads',
    () {
      final profile = MachineProfile.fromHealth({
        'profile': 'core',
        'services': ['accounting'],
      });
      final refused = profile.refused(['accounting', 'budget', 'multimodal']);
      expect(refused, ['budget', 'multimodal']);
    },
  );
}
