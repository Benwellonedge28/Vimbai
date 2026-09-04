// Book context: the currently selected shared Book (personal, household,
// group, business, nonprofit or an organization's Book). All service
// clients attach X-Book-ID so every service call runs inside the chosen
// context - the gateway verifies membership and injects the role
// downstream (X-Book-Role / X-Book-Tier).

class BookContextBook {
  final String id;
  final String name;
  final String tier;
  final String yourRole;
  final String? source; // 'sync' (book-sync) or 'org' (npo-scale org Book)

  BookContextBook({
    required this.id,
    required this.name,
    required this.tier,
    required this.yourRole,
    this.source,
  });

  factory BookContextBook.fromJson(Map<String, dynamic> j, {String? source}) {
    return BookContextBook(
      id: j['id'] as String,
      name: (j['name'] as String?) ?? 'Book',
      tier: (j['tier'] as String?) ?? 'personal',
      yourRole:
          (j['your_role'] as String?) ?? (j['role'] as String?) ?? 'viewer',
      source: source,
    );
  }
}

class BookContext {
  BookContext._();
  static final BookContext instance = BookContext._();

  BookContextBook? _current;
  final List<void Function()> _listeners = [];

  BookContextBook? get current => _current;
  bool get hasContext => _current != null;

  /// Headers every service client should attach so requests run inside the
  /// selected Book context.
  Map<String, String> headers() {
    final b = _current;
    if (b == null) return {};
    return {'X-Book-ID': b.id};
  }

  void setBook(BookContextBook book) {
    _current = book;
    for (final l in _listeners) {
      l();
    }
  }

  void clear() {
    _current = null;
    for (final l in _listeners) {
      l();
    }
  }

  void addListener(void Function() l) => _listeners.add(l);
  void removeListener(void Function() l) => _listeners.remove(l);
}
