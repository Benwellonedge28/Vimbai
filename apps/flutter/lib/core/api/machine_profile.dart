// What the machine this app is talking to actually serves.
//
// A single machine can run the whole platform or only the bookkeeping loop: `vimbai-local --profile core` answers accounting,
// departmental accounting, the audit trail and the Books, and refuses every other prefix with a 404 that names it. The app
// could not tell the difference, so it offered a bank feed and a cash-flow statement on a machine that would refuse both.
//
// Nothing here decides what to *show*; it answers one question - does this machine serve this prefix - so a screen can ask it
// instead of discovering the answer from a refusal.

/// The profile a machine reports, and the prefixes it says it serves.
class MachineProfile {
  const MachineProfile({required this.profile, required this.members});

  /// What `/health` called itself: `core`, `full`, or whatever a later runtime reports.
  final String profile;

  /// The prefixes it serves, without their leading slash, as `/health` reports them.
  final Set<String> members;

  /// A machine that serves everything, which is what the app assumes when it cannot ask.
  ///
  /// The assumption is deliberate: hiding a working feature from a caller of an engine that never answered `/health` is worse
  /// than offering one a machine refuses, because the refusal is at least a sentence the caller can read.
  static const MachineProfile everything = MachineProfile(
    profile: 'full',
    members: <String>{},
  );

  /// Reads the health body. An unexpected shape is not an error: it is a machine whose profile the app does not recognise,
  /// which serves everything as far as this app is concerned.
  factory MachineProfile.fromHealth(Object? body) {
    if (body is! Map) {
      return everything;
    }
    final profile = body['profile'];
    if (profile is! String) {
      return everything;
    }
    final members = <String>{};
    final served = body['services'];
    if (served is List) {
      for (final member in served) {
        if (member is String && member.isNotEmpty) {
          members.add(member);
        }
      }
    }
    return MachineProfile(profile: profile, members: members);
  }

  /// Does this machine serve [prefix], named the way the app names it (`accounting`, `/accounting`)?
  bool serves(String prefix) {
    if (profile == 'full') {
      return true;
    }
    return members.contains(prefix.replaceAll('/', ''));
  }

  /// The features this machine will refuse, in the words a person reads.
  List<String> refused(List<String> features) =>
      features.where((feature) => !serves(feature)).toList();
}
