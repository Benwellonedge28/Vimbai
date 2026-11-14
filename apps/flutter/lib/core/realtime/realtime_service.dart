// lib/services/realtime_service.dart
//
// Live Book events, pushed rather than polled.
//
// The backend already publishes a `book.resource.*` event for every successful
// write inside a Book, and the WebSocket service fans those events out to the
// sockets subscribed to that Book's channel. This client opens one such socket
// per signed-in session so a second device sees a journal entry, invoice or
// inbox record appear without waiting for a poll.
//
// It is deliberately defensive: a dropped socket is normal on mobile (background,
// tunnel, roaming). The service reconnects with capped backoff, exposes
// `connected` so screens can fall back to a slow poll while it is down, and
// never throws into the widget tree.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:vimbai_mobile_client/app/config.dart';
import 'package:vimbai_mobile_client/core/local_db/user_local_data.dart';
import 'package:vimbai_mobile_client/features/books/data/book_context.dart';

/// One event as the WebSocket service sends it.
@immutable
class RealtimeEvent {
  final String? eventId;
  final String? eventType;
  final String? bookId;
  final String? resourceType;
  final String? resourceId;
  final String? action;
  final String? sourceService;
  final DateTime receivedAt;
  final Map<String, dynamic> payload;

  const RealtimeEvent({
    required this.eventId,
    required this.eventType,
    required this.bookId,
    required this.resourceType,
    required this.resourceId,
    required this.action,
    required this.sourceService,
    required this.receivedAt,
    required this.payload,
  });

  /// Parse a fan-out frame. Returns null for frames that are not Book events
  /// (the connection hello, disconnects, echoes of our own writes).
  static RealtimeEvent? tryParse(Map<String, dynamic> frame) {
    if (frame['type'] != 'event') return null;
    final payload = (frame['payload'] as Map?)?.cast<String, dynamic>() ?? {};
    return RealtimeEvent(
      eventId: frame['event_id'] as String?,
      eventType: frame['event_type'] as String?,
      bookId: frame['book_id'] as String?,
      resourceType: (frame['resource_type'] ?? payload['resource_type']) as String?,
      resourceId: (frame['resource_id'] ?? payload['resource_id']) as String?,
      action: frame['action'] as String?,
      sourceService: frame['source_service'] as String?,
      receivedAt: DateTime.tryParse((frame['timestamp'] as String?) ?? '')?.toLocal() ?? DateTime.now(),
      payload: payload,
    );
  }

  /// Any change worth re-reading a screen for.
  bool get isRecordChange =>
      resourceType != null || (eventType?.startsWith('book.resource.') ?? false);
}

class RealtimeService {
  RealtimeService._();

  static final RealtimeService instance = RealtimeService._();

  static const Duration _minBackoff = Duration(seconds: 2);
  static const Duration _maxBackoff = Duration(minutes: 1);

  final StreamController<RealtimeEvent> _events = StreamController<RealtimeEvent>.broadcast();
  final ValueNotifier<bool> _connected = ValueNotifier<bool>(false);

  WebSocket? _socket;
  StreamSubscription<dynamic>? _subscription;
  Timer? _reconnect;
  Duration _backoff = _minBackoff;
  bool _stopping = false;
  String? _channel;

  /// Live events for the current Book. Broadcast: many screens may listen.
  Stream<RealtimeEvent> get events => _events.stream;

  /// Whether the socket is currently up. Screens poll slowly while this is false.
  ValueListenable<bool> get connected => _connected;
  bool get isConnected => _connected.value;

  /// Subscribe to the current Book's channel. Safe to call repeatedly; a call
  /// for the same Book is a no-op, a call for a different Book re-subscribes.
  Future<void> start() async {
    _stopping = false;
    final bookId = BookContext.instance.current?.id;
    if (bookId == null) return; // No Book yet: nothing to listen to.
    final channel = 'book:$bookId';
    if (channel == _channel && (_socket != null || _reconnect != null)) {
      return;
    }
    _channel = channel;
    await _connect();
  }

  /// Stop and forget the current channel (logout, Book switch, app pause).
  Future<void> stop() async {
    _stopping = true;
    _reconnect?.cancel();
    _reconnect = null;
    _channel = null;
    await _teardown();
    _setConnected(false);
  }

  Future<void> _connect() async {
    if (_stopping || _channel == null) return;
    await _teardown();

    final token = await UserLocalData.getAuthToken();
    final bookId = BookContext.instance.current?.id;
    if (token == null || bookId == null) {
      _scheduleReconnect();
      return;
    }

    final uri = Uri.parse('${AppConfig.realtimeUrl}/ws/$_channel');
    try {
      final socket = await WebSocket.connect(
        uri.toString(),
        headers: {
          'Authorization': 'Bearer $token',
          'X-Book-ID': bookId,
        },
      ).timeout(AppConfig.connectTimeout);
      _socket = socket;
      _setConnected(true);
      _backoff = _minBackoff;

      _subscription = socket.listen(
        _onFrame,
        onError: (_) => _handleDrop(),
        onDone: _handleDrop,
        cancelOnError: true,
      );
    } catch (_) {
      _setConnected(false);
      _scheduleReconnect();
    }
  }

  void _onFrame(dynamic raw) {
    if (raw is! String) return;
    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map) return;
      final event = RealtimeEvent.tryParse(decoded.cast<String, dynamic>());
      if (event != null && !_events.isClosed) _events.add(event);
    } catch (_) {
      // A malformed frame is not worth a reconnect; the slow poll covers gaps.
    }
  }

  void _handleDrop() {
    _setConnected(false);
    _scheduleReconnect();
  }

  void _scheduleReconnect() {
    if (_stopping || _channel == null || _reconnect != null) return;
    final delay = _backoff;
    _backoff = _backoff * 2 > _maxBackoff ? _maxBackoff : _backoff * 2;
    _reconnect = Timer(delay, () async {
      _reconnect = null;
      await _connect();
    });
  }

  Future<void> _teardown() async {
    await _subscription?.cancel();
    _subscription = null;
    try {
      await _socket?.close();
    } catch (_) {
      // Already gone.
    }
    _socket = null;
  }

  void _setConnected(bool value) {
    if (_connected.value != value) _connected.value = value;
  }

  /// Test seam: feed a frame as if the socket had delivered it.
  @visibleForTesting
  void debugHandleFrame(String raw) => _onFrame(raw);
}
