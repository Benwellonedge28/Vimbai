// Proof that nothing in the books has been changed since it was written.
//
// Every posted entry carries a fingerprint of its own contents and of the entry before it, so the entries form a chain: change
// any figure in a past entry and its fingerprint no longer matches, and the entry after it points at a predecessor that no
// longer exists. The engine walks that chain on request and says whether every link holds.
//
// Two states look alike and are not: an empty book has nothing to prove, and an intact book has been proved. Nothing here may
// turn "no entries" into "verified".
/// One thing the walk found wrong, in the engine's own words and the app's.
class IntegrityProblem {
  const IntegrityProblem({required this.code, required this.entryId});

  /// The engine's code, e.g. `hash_mismatch` or `broken_link`.
  final String code;

  /// The entry the problem is at. A person takes this to whoever looks after the books.
  final String entryId;

  /// What it means, said without jargon. An unknown code is passed on as itself rather than guessed at.
  String get meaning => switch (code) {
    'hash_mismatch' =>
      'Entry $entryId no longer matches the fingerprint written when it was saved, '
          'so it was changed after it was posted.',
    'broken_link' =>
      'Entry $entryId was saved expecting a different entry before it, so one of the '
          'two has moved.',
    _ => 'The engine reported "$code" at entry $entryId.',
  };

  factory IntegrityProblem.fromJson(Map<String, dynamic> json) {
    final detail = json['detail'];
    final entryId = detail is Map ? (detail['entry_id'] ?? '').toString() : '';
    return IntegrityProblem(
      code: (json['code'] ?? 'unknown').toString(),
      entryId: entryId,
    );
  }
}

/// The engine's answer to `GET /ledger/integrity`.
class LedgerIntegrity {
  const LedgerIntegrity({
    required this.valid,
    required this.entriesChecked,
    required this.headHash,
    required this.problems,
    required this.legacyStamped,
  });

  final bool valid;
  final int entriesChecked;

  /// The fingerprint of the newest entry, which is what the next entry will be chained from.
  final String headHash;

  final List<IntegrityProblem> problems;

  /// Entries written before the fingerprint covered the exact decimal amounts. They verify, they are simply older.
  final int legacyStamped;

  /// Nothing has been posted, so this check proves nothing either way. Not the same as intact.
  bool get isProofOfNothing => entriesChecked == 0 && valid;

  bool get isIntact => valid && !isProofOfNothing;

  /// The head fingerprint, shortened for reading. The whole thing is kept for copying.
  String get shortHeadHash =>
      headHash.length <= 12 ? headHash : '${headHash.substring(0, 12)}…';

  factory LedgerIntegrity.fromJson(Map<String, dynamic> json) =>
      LedgerIntegrity(
        valid: json['valid'] == true,
        entriesChecked: (json['entries_checked'] as num?)?.toInt() ?? 0,
        headHash: (json['head_hash'] ?? '').toString(),
        problems: [
          for (final problem in (json['errors'] as List? ?? const []))
            if (problem is Map<String, dynamic>)
              IntegrityProblem.fromJson(problem),
        ],
        legacyStamped: (json['legacy_stamped'] as List? ?? const []).length,
      );
}
