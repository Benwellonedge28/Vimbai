// Saying, once, which features this machine will refuse.
//
// A core install answers the bookkeeping loop and refuses the rest. Rather than let a caller tap into a screen that comes back
// 404, the app reads the profile at startup and names what is not here.

import 'package:flutter/material.dart';

import '../core/api/machine_profile.dart';

class ProfileBanner extends StatelessWidget {
  const ProfileBanner({
    super.key,
    required this.profile,
    required this.features,
  });

  /// What the machine says it serves.
  final MachineProfile profile;

  /// The features worth naming, in the words the screens use: `banking-integration`, `cash-flow-statement`.
  final Map<String, String> features;

  @override
  Widget build(BuildContext context) {
    final refused = features.entries
        .where((entry) => !profile.serves(entry.key))
        .map((entry) => entry.value)
        .toList();
    if (refused.isEmpty) {
      return const SizedBox.shrink();
    }
    final names = refused.length == 1
        ? refused.single
        : '${refused.sublist(0, refused.length - 1).join(', ')} and ${refused.last}';
    return Card(
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
      child: ListTile(
        leading: const Icon(Icons.info_outline),
        title: Text('This machine runs the ${profile.profile} profile'),
        subtitle: Text(
          '$names ${refused.length == 1 ? 'is' : 'are'} not available here.',
        ),
      ),
    );
  }
}
