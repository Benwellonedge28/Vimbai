class User {
  final String id;
  final String username;
  final String email;
  final String role;
  final List<String> permissions;

  User({
    required this.id,
    required this.username,
    required this.email,
    required this.role,
    this.permissions = const [],
  });

  factory User.fromJson(Map<String, dynamic> json) {
    return User(
      id: json['id'],
      username: json['username'],
      email: json['email'],
      role: json['role'],
      permissions: List<String>.from(json['permissions'] ?? []),
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'username': username,
      'email': email,
      'role': role,
      'permissions': permissions,
    };
  }
}
