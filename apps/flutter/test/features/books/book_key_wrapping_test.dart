// A Book key must leave the phone in a form only this phone can open.
//
// The server holds each member's copy of a Book key. The first version of the app handed it the key base64-encoded behind a
// "self:" mark, which is the key itself: anyone who could read the store could read every Book. A device key pair now
// wraps it, and a key wrapped the old way still opens so existing Books are not lost.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';

void main() {
  final service = BookSyncService.instance;
  final bookKey = base64Encode(List<int>.generate(32, (i) => 200 - i));

  late Directory deviceDir;

  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
    // its own directory per test file: two files sharing one store fight over the locks
    deviceDir = Directory.systemTemp.createTempSync('vimbai_books_');
    BookSyncService.databasesDirectory = deviceDir.path;
  });

  tearDownAll(() {
    BookSyncService.databasesDirectory = null;
    if (deviceDir.existsSync()) deviceDir.deleteSync(recursive: true);
  });

  test('a Book key wrapped for this device opens again', () async {
    final wrapped = await service.wrapBookKeyForSelf(bookKey);
    expect(await service.unwrapBookKey(wrapped), bookKey);
  }, timeout: const Timeout(Duration(minutes: 2)));

  test('the wrapped form does not carry the key in a readable form', () async {
    final wrapped = await service.wrapBookKeyForSelf(bookKey);
    expect(wrapped.contains(bookKey), isFalse);
    expect(wrapped.startsWith('self:'), isFalse);
  });

  test('the same key wrapped twice gives two different ciphertexts', () async {
    final first = await service.wrapBookKeyForSelf(bookKey);
    final second = await service.wrapBookKeyForSelf(bookKey);
    expect(first, isNot(equals(second)));
  }, timeout: const Timeout(Duration(minutes: 2)));

  test('a key wrapped the old way still opens', () async {
    final legacy = 'self:${base64Encode(utf8.encode(bookKey))}';
    expect(await service.unwrapBookKey(legacy), bookKey);
  });
}
