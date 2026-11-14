// What the app reads off the wire and what it writes back have to agree, including when the server leaves a field out.
import 'package:flutter_test/flutter_test.dart';
import 'package:vimbai_mobile_client/features/authentication/models/user.dart';
import 'package:vimbai_mobile_client/features/books/models/book_models.dart';

void main() {
  test('a person survives the trip to JSON and back', () {
    final person = User(
      id: 'u1',
      username: 'sam',
      email: 'sam@example.test',
      role: 'owner',
      permissions: const ['books:read', 'books:write'],
    );

    final back = User.fromJson(person.toJson());

    expect(back.id, 'u1');
    expect(back.username, 'sam');
    expect(back.role, 'owner');
    expect(back.permissions, ['books:read', 'books:write']);
  });

  test('a person with no permissions listed reads as a person with none', () {
    final back = User.fromJson({
      'id': 'u2',
      'username': 'no-permissions',
      'email': 'n@example.test',
      'role': 'viewer',
    });

    expect(back.permissions, isEmpty);
  });

  test('a Book reads the fields the engine sends, including the sequence number', () {
    final book = VBook.fromJson({
      'id': 'b1',
      'name': 'Household',
      'tier': 'household',
      'your_role': 'member',
      'seq': 42,
      'created_at': 1700000000,
    });

    expect(book.name, 'Household');
    expect(book.yourRole, 'member');
    expect(book.seq, 42, reason: 'the sequence number is what a device resumes from');
    expect(book.createdAt!.toUtc().year, 2023);
  });

  test('a Book with fields missing falls back instead of throwing', () {
    final book = VBook.fromJson({'id': 'b2'});

    expect(book.name, '');
    expect(book.tier, 'personal');
    expect(book.seq, 0);
    expect(book.createdAt, isNull);
  });
}
