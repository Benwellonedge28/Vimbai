// Book models for the Vimbai multi-audience Book system.
//
// A Book (Space) is the atomic unit for one audience: personal,
// household, group, business or nonprofit (fund accounting). A user can hold different roles in
// many Books on the same device - privileges are per-membership,
// never global (book-design.md Ch. 35).

class VBook {
  final String id;
  final String name;
  final String tier; // personal | household | group | business
  final String description;
  final String yourRole;
  final String membershipStatus; // active | invited
  final int seq;
  final DateTime? createdAt;

  VBook({
    required this.id,
    required this.name,
    required this.tier,
    this.description = '',
    this.yourRole = 'owner',
    this.membershipStatus = 'active',
    this.seq = 0,
    this.createdAt,
  });

  factory VBook.fromJson(Map<String, dynamic> j) {
    return VBook(
      id: j['id'] as String,
      name: j['name'] as String? ?? '',
      tier: j['tier'] as String? ?? 'personal',
      description: j['description'] as String? ?? '',
      yourRole: j['your_role'] as String? ?? 'viewer',
      membershipStatus: j['membership_status'] as String? ?? 'active',
      seq: (j['seq'] as num?)?.toInt() ?? 0,
      createdAt: j['created_at'] == null
          ? null
          : DateTime.fromMillisecondsSinceEpoch(
              ((j['created_at'] as num) * 1000).round(),
            ),
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'tier': tier,
        'description': description,
        'your_role': yourRole,
        'membership_status': membershipStatus,
        'seq': seq,
      };
}

class BookMember {
  final String userId;
  final String role;
  final String displayName;
  final String status;

  BookMember({
    required this.userId,
    required this.role,
    this.displayName = '',
    this.status = 'active',
  });

  factory BookMember.fromJson(Map<String, dynamic> j) {
    return BookMember(
      userId: j['user_id'] as String? ?? '',
      role: j['role'] as String? ?? 'viewer',
      displayName: j['display_name'] as String? ?? '',
      status: j['status'] as String? ?? 'active',
    );
  }
}

/// Roles ordered most -> least privileged. Used by the UI to decide
/// which actions the active membership permits.
const List<String> kBookRoleLadder = [
  'owner',
  'admin',
  'treasurer',
  'bookkeeper',
  'viewer',
  'auditor',
];

const List<String> kBookWriterRoles = [
  'owner',
  'admin',
  'treasurer',
  'bookkeeper',
];

const List<String> kBookTiers = [
  'personal',
  'household',
  'group',
  'business',
  'nonprofit',
];
