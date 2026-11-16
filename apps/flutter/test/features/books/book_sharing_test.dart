// Sharing a Book means handing one person the key in a form only they can open.
//
// The invitation used to carry the text "wrapped:<user id>", which is not a key: the engine accepted it, the person joined,
// and every entry they pulled failed to decrypt. These tests follow the whole way out - the invitee's public key is fetched,
// the Book key is wrapped for it, and the thing posted is openable with the invitee's private key and by nobody else.
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:pointycastle/export.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/core/api/api_client.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';

AsymmetricKeyPair<PublicKey, PrivateKey> _person() {
  final random = FortunaRandom()
    ..seed(
      KeyParameter(
        Uint8List.fromList(
          List<int>.generate(32, (_) => Random.secure().nextInt(256)),
        ),
      ),
    );
  final generator = RSAKeyGenerator()
    ..init(
      ParametersWithRandom(
        RSAKeyGeneratorParameters(BigInt.from(65537), 2048, 64),
        random,
      ),
    );
  return generator.generateKeyPair();
}

String _b64(BigInt value) {
  final bytes = <int>[];
  var rest = value;
  while (rest > BigInt.zero) {
    bytes.insert(0, (rest & BigInt.from(0xFF)).toInt());
    rest = rest >> 8;
  }
  return base64Encode(bytes.isEmpty ? [0] : bytes);
}

BigInt _fromB64Bytes(String b64) {
  var value = BigInt.zero;
  for (final byte in base64Decode(b64)) {
    value = (value << 8) | BigInt.from(byte);
  }
  return value;
}

void main() {
  final service = BookSyncService.instance;
  late Directory deviceDir;
  final originalTransport = ApiClient.transport;

  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
    deviceDir = Directory.systemTemp.createTempSync('vimbai_share_');
    BookSyncService.databasesDirectory = deviceDir.path;
  });

  tearDownAll(() {
    BookSyncService.databasesDirectory = null;
    if (deviceDir.existsSync()) deviceDir.deleteSync(recursive: true);
  });

  tearDown(() {
    ApiClient.transport = originalTransport;
  });

  test(
    'an invitation carries the Book key made openable for that person',
    () async {
      final invitee = _person();
      final inviteePublic = invitee.publicKey as RSAPublicKey;
      Map<String, dynamic>? posted;
      var published = false;

      ApiClient.transport = MockClient((request) async {
        final json = {'content-type': 'application/json'};
        if (request.url.path.endsWith('/identity/users/me/public-key')) {
          published = true;
          return http.Response('{}', 200, headers: json);
        }
        if (request.url.path.contains('/identity/users/') &&
            request.url.path.endsWith('/public-key')) {
          return http.Response(
            jsonEncode({
              'user_id': 'them',
              'modulus': _b64(inviteePublic.modulus!),
              'exponent': _b64(inviteePublic.exponent!),
            }),
            200,
            headers: json,
          );
        }
        if (request.url.path.endsWith('/books/book-1/members')) {
          posted = jsonDecode(request.body) as Map<String, dynamic>;
          return http.Response('{"member":{}}', 200, headers: json);
        }
        if (request.url.path.endsWith('/devices')) {
          return http.Response('{"device_id":"device-1"}', 200, headers: json);
        }
        return http.Response(
          jsonEncode({
            'book': {
              'id': 'book-1',
              'name': 'Mukandara Books',
              'tier': 'household',
              'description': '',
              'folder': '',
              'role': 'owner',
              'status': 'active',
            },
          }),
          200,
          headers: json,
        );
      });
      service.setAuthToken('a-token');

      final book = await service.createBook('Mukandara Books', 'household');
      expect(
        published,
        isTrue,
        reason: 'a shareable Book publishes its key first',
      );

      await service.inviteMember(book.id, 'them', 'member');

      expect(posted, isNotNull);
      final wrapped = posted!['wrapped_book_key'] as String;
      expect(wrapped.startsWith('wrapped:'), isFalse);

      // only their private key opens it, and what it opens is the Book key this phone holds
      final engine = OAEPEncoding(RSAEngine())
        ..init(
          false,
          PrivateKeyParameter<RSAPrivateKey>(
            invitee.privateKey as RSAPrivateKey,
          ),
        );
      final opened = utf8.decode(
        engine.process(Uint8List.fromList(base64Decode(wrapped))),
      );
      expect(opened, await service.bookKeyFor(book.id));
      expect(opened.length, greaterThan(20));
    },
  );

  test(
    'inviting someone who never opened a phone says so instead of posting a key',
    () async {
      var posted = false;
      ApiClient.transport = MockClient((request) async {
        final json = {'content-type': 'application/json'};
        if (request.url.path.contains('/identity/users/') &&
            request.url.path.endsWith('/public-key')) {
          return http.Response(
            '{"detail":"No public key published"}',
            404,
            headers: json,
          );
        }
        if (request.url.path.endsWith('/members')) {
          posted = true;
          return http.Response('{}', 200, headers: json);
        }
        if (request.url.path.endsWith('/devices')) {
          return http.Response('{"device_id":"device-1"}', 200, headers: json);
        }
        return http.Response(
          jsonEncode({
            'book': {
              'id': 'book-1',
              'name': 'Mukandara Books',
              'tier': 'household',
              'description': '',
              'folder': '',
              'role': 'owner',
              'status': 'active',
            },
          }),
          200,
          headers: json,
        );
      });
      service.setAuthToken('a-token');
      final book = await service.createBook('Mukandara Books', 'household');

      await expectLater(
        service.inviteMember(book.id, 'stranger', 'member'),
        throwsA(
          predicate((e) => e.toString().contains('has not opened Vimbai')),
        ),
      );
      expect(
        posted,
        isFalse,
        reason: 'nothing is posted that the invitee could not open',
      );
    },
  );

  test(
    'a name is resolved before anything is wrapped, and a wrong name posts nothing',
    () async {
      final invitee = _person();
      final inviteePublic = invitee.publicKey as RSAPublicKey;
      final asked = <String>[];
      var posted = false;

      ApiClient.transport = MockClient((request) async {
        final json = {'content-type': 'application/json'};
        if (request.url.path.endsWith('/identity/users/lookup')) {
          asked.add(request.url.queryParameters['username'] ?? '');
          if (request.url.queryParameters['username'] == 'them') {
            return http.Response(
              jsonEncode({
                'id': 'their-id',
                'username': 'them',
                'full_name': 'Tariro Moyo',
              }),
              200,
              headers: json,
            );
          }
          return http.Response(
            '{"detail":"No account with that username"}',
            404,
            headers: json,
          );
        }
        if (request.url.path.contains('/identity/users/') &&
            request.url.path.endsWith('/public-key')) {
          return http.Response(
            jsonEncode({
              'user_id': 'their-id',
              'modulus': _b64(inviteePublic.modulus!),
              'exponent': _b64(inviteePublic.exponent!),
            }),
            200,
            headers: json,
          );
        }
        if (request.url.path.endsWith('/members')) {
          posted = true;
          return http.Response('{"member":{}}', 200, headers: json);
        }
        if (request.url.path.endsWith('/devices')) {
          return http.Response('{"device_id":"device-1"}', 200, headers: json);
        }
        return http.Response(
          jsonEncode({
            'book': {
              'id': 'book-1',
              'name': 'Mukandara Books',
              'tier': 'household',
              'description': '',
              'folder': '',
              'role': 'owner',
              'status': 'active',
            },
          }),
          200,
          headers: json,
        );
      });
      service.setAuthToken('a-token');
      final book = await service.createBook('Mukandara Books', 'household');

      final person = await service.inviteMemberByUsername(
        book.id,
        'them',
        'member',
      );
      expect(person['full_name'], 'Tariro Moyo');
      expect(posted, isTrue);
      expect(asked, ['them']);

      posted = false;
      await expectLater(
        service.inviteMemberByUsername(book.id, 'typo', 'member'),
        throwsA(predicate((e) => e.toString().contains('No active account'))),
      );
      expect(
        posted,
        isFalse,
        reason: 'a name that resolves to nobody posts nothing at all',
      );
    },
  );

  test(
    'a person who accepted an invitation holds a key this phone can open and use',
    () async {
      // the two halves have only been proven apart until here: the inviter wraps for a published key, and the invitee opens
      // whatever arrives. If those two disagree about the key - a different modulus, a different padding - the member joins
      // and can read nothing, which is the exact failure this whole door exists to prevent.
      var wrappedForMe = '';
      String? myModulus;
      String? myExponent;

      ApiClient.transport = MockClient((request) async {
        final json = {'content-type': 'application/json'};
        if (request.url.path.endsWith('/identity/users/me/public-key')) {
          final sent = jsonDecode(request.body) as Map<String, dynamic>;
          myModulus = sent['modulus'] as String;
          myExponent = sent['exponent'] as String;
          return http.Response('{}', 200, headers: json);
        }
        if (request.url.path.endsWith('/books')) {
          return http.Response(
            jsonEncode({
              'books': [
                {
                  'id': 'book-2',
                  'name': 'Family Books',
                  'tier': 'household',
                  'description': '',
                  'folder': '',
                  'your_role': 'bookkeeper',
                  'membership_status': 'invited',
                },
              ],
            }),
            200,
            headers: json,
          );
        }
        if (request.url.path.endsWith('/books/book-2/members/accept')) {
          return http.Response(
            jsonEncode({'wrapped_book_key': wrappedForMe}),
            200,
            headers: json,
          );
        }
        return http.Response('{}', 200, headers: json);
      });
      service.setAuthToken('a-token');

      await service.publishDevicePublicKey();
      expect(
        myModulus,
        isNotNull,
        reason: 'this phone publishes the key others wrap for',
      );

      // the inviter's side, done here with the key this phone actually published
      const bookKey = 'Zm9yLXRlc3Rpbmctb25seS1hZXMtMjU2LWtleS0xMjM0';
      final engine = OAEPEncoding(RSAEngine())
        ..init(
          true,
          PublicKeyParameter<RSAPublicKey>(
            RSAPublicKey(_fromB64Bytes(myModulus!), _fromB64Bytes(myExponent!)),
          ),
        );
      wrappedForMe = base64Encode(
        engine.process(Uint8List.fromList(utf8.encode(bookKey))),
      );

      final books = await service.refreshBooksFromServer();
      expect(books.single.id, 'book-2');
      expect(books.single.membershipStatus, 'invited');

      await service.acceptInvite('book-2');
      expect(
        await service.bookKeyFor('book-2'),
        bookKey,
        reason: 'the key stored after accepting is the one the inviter wrapped',
      );
    },
  );
}
