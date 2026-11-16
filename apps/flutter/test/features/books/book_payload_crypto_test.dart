// What a Book's payloads must do before anything is written or synced with them.
//
// Every payload is sealed with the Book key on the device. Two rules matter: sealing the same data twice must never
// produce the same ciphertext (a repeated GCM nonce hands an observer the exclusive-or of the plaintexts and the material
// to forge tags), and a Book written before the nonce was fixed must still open.
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:pointycastle/export.dart';
import 'package:vimbai_mobile_client/features/books/data/book_sync_service.dart';

void main() {
  final service = BookSyncService.instance;
  final key = base64Encode(List<int>.generate(32, (i) => i + 1));
  const payload = {'amount': '125.00', 'memo': 'milk and towels'};

  test('a sealed payload opens again', () {
    final sealed = service.encryptPayload(key, payload);
    expect(service.decryptPayload(key, sealed), payload);
  });

  test('the same data sealed twice is never the same ciphertext', () {
    final first = service.encryptPayload(key, payload);
    final second = service.encryptPayload(key, payload);
    expect(first, isNot(equals(second)));
  });

  test('a payload written under the old fixed nonce still opens', () {
    // exactly what the previous version produced: GCM under twelve zero bytes, no marker, nothing else
    final gcm = GCMBlockCipher(AESEngine())
      ..init(
        true,
        AEADParameters(
          KeyParameter(Uint8List.fromList(base64Decode(key))),
          128,
          Uint8List(12),
          Uint8List(0),
        ),
      );
    final legacy = base64Encode(
      gcm.process(Uint8List.fromList(utf8.encode(jsonEncode(payload)))),
    );

    expect(service.decryptPayload(key, legacy), payload);
  });

  test('a payload altered in flight does not open', () {
    final sealed = service.encryptPayload(key, payload);
    final bytes = base64Decode(sealed);
    bytes[bytes.length - 1] = bytes[bytes.length - 1] ^ 0x01;

    expect(
      () => service.decryptPayload(key, base64Encode(bytes)),
      throwsA(anything),
    );
  });

  test('a payload sealed under one Book key does not open under another', () {
    final other = base64Encode(List<int>.generate(32, (_) => 7));
    final sealed = service.encryptPayload(key, payload);

    expect(() => service.decryptPayload(other, sealed), throwsA(anything));
  });
}
