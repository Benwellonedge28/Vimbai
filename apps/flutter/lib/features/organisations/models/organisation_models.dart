// What an organisation looks like as the engine answers it.
//
// An organisation is the thing that owns books: one person may belong to several, a book belongs to at most one, and the
// membership - not the book - is what every later question about access reads. The app only needs enough of it to name it,
// list it and put a book inside it, so that is all this holds.

class VOrganisation {
  const VOrganisation({
    required this.id,
    required this.name,
    this.slug = '',
    this.role = 'member',
    this.bookCount = 0,
  });

  final String id;
  final String name;
  final String slug;

  /// What the signed-in person is in this organisation: owner, admin or member.
  final String role;

  /// How many Books sit inside it, when the engine says.
  final int bookCount;

  /// Whether this person may add or move a Book here, which needs both halves: administering the organisation and holding
  /// the Book. Read off the role the engine gave rather than guessed from a separate permission list, because the engine
  /// answers the role with the organisation and nothing else.
  bool get canManageBooks => role == 'owner' || role == 'admin';

  factory VOrganisation.fromJson(Map<String, dynamic> json) {
    return VOrganisation(
      id: json['id'] as String? ?? '',
      name: json['name'] as String? ?? '',
      slug: json['slug'] as String? ?? '',
      role: json['role'] as String? ?? 'member',
      bookCount: (json['book_count'] as num?)?.toInt() ?? 0,
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'name': name,
    'slug': slug,
    'role': role,
    'book_count': bookCount,
  };
}
