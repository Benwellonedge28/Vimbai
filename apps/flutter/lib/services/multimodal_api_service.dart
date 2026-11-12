// mobile-client/lib/services/multimodal_api_service.dart

import 'dart:convert';
import 'dart:io';
import 'package:vimbai_mobile_client/config.dart';
import 'package:vimbai_mobile_client/services/api_client.dart';
import 'package:vimbai_mobile_client/models/multimodal_models.dart'; // Ensure these models exist or are defined
import 'package:vimbai_mobile_client/services/auth_service.dart'; // For getting authentication token
import 'package:vimbai_mobile_client/services/book_context.dart';
import 'package:vimbai_mobile_client/local_db/database_helper.dart';

class MultimodalApiService {
  final String _baseUrl = AppConfig.multimodalRoute; // Via API Gateway (offline-aware client below)
  final ApiClient _client = ApiClient();
  final AuthService _authService = AuthService();
  final DatabaseHelper _localDb = DatabaseHelper();

  Future<Map<String, String>> _getHeaders() async {
    final token = await _authService.getToken();
    return {
      'Content-Type': 'application/json',
      if (token != null) 'Authorization': 'Bearer $token',
      ...BookContext.instance.headers(),
    };
  }

  // --- High-level convenience methods (used by the input page) ---

  /// Submits an image/document for OCR processing and waits for the
  /// backend pipeline to produce a [DocumentParseResult].
  Future<DocumentParseResult> processDocumentOcr({
    required File imageFile,
    String? sourceContext,
  }) async {
    final dataUrl = await _fileToDataUrl(imageFile);
    final task = await createTask(MultimodalProcessingTaskCreate(
      userId: 'self',
      inputType: MultimodalInputType.document,
      inputUrl: dataUrl,
      metadata: {'source_context': sourceContext ?? 'mobile'},
    ));
    final done = await _pollTask(task.id);
    if (done?.documentResult != null) return done!.documentResult!;
    throw Exception('Document processing did not complete in time');
  }

  /// Submits an audio file for transcription and waits for the backend
  /// pipeline to produce an [AudioParseResult].
  Future<AudioParseResult> processAudioToText({
    required File audioFile,
    String? sourceContext,
  }) async {
    final dataUrl = await _fileToDataUrl(audioFile);
    final task = await createTask(MultimodalProcessingTaskCreate(
      userId: 'self',
      inputType: MultimodalInputType.audio,
      inputUrl: dataUrl,
      metadata: {'source_context': sourceContext ?? 'mobile'},
    ));
    final done = await _pollTask(task.id);
    if (done?.audioResult != null) return done!.audioResult!;
    throw Exception('Audio processing did not complete in time');
  }

  /// Encodes a local file as a base64 data URL (the pipeline accepts
  /// raw image/audio data or URLs).
  Future<String> _fileToDataUrl(File file) async {
    final bytes = await file.readAsBytes();
    final mime = file.path.endsWith('.wav')
        ? 'audio/wav'
        : file.path.endsWith('.mp3')
            ? 'audio/mpeg'
            : file.path.endsWith('.m4a')
                ? 'audio/mp4'
                : file.path.endsWith('.mp4')
                    ? 'video/mp4'
                    : file.path.endsWith('.mov')
                        ? 'video/quicktime'
                        : 'image/jpeg';
    final b64 = base64Encode(bytes);
    return 'data:$mime;base64,$b64';
  }

  /// Polls a task until it reaches a terminal status or times out
  /// (~20s). Returns the latest task snapshot either way.
  Future<MultimodalProcessingTaskInDB?> _pollTask(String taskId,
      {int maxAttempts = 10}) async {
    for (var i = 0; i < maxAttempts; i++) {
      final task = await getTask(taskId);
      if (task.status == MultimodalProcessingStatus.completed ||
          task.status == MultimodalProcessingStatus.failed ||
          task.status == MultimodalProcessingStatus.reviewPending) {
        return task;
      }
      await Future.delayed(const Duration(seconds: 2));
    }
    try {
      return await getTask(taskId);
    } catch (_) {
      return null;
    }
  }

  // --- Offline capture queue ---

  /// Queues a capture on-device when the network is down. SyncService
  /// replays it (with the same payload) once connectivity returns.
  Future<void> _queueOfflineCapture({
    required MultimodalInputType inputType,
    String? dataUrl,
    String? rawText,
    required Map<String, dynamic> metadata,
  }) async {
    final now = DateTime.now();
    await _localDb.insertMultimodalTask(
      MultimodalInput(
        id: 'local-${now.microsecondsSinceEpoch}',
        userId: 'self',
        inputType: inputType,
        dataUrl: dataUrl,
        rawText: rawText,
        metadata: metadata,
        createdAt: now,
        updatedAt: now,
      ),
      isSynced: false,
    );
  }

  /// A stand-in task so capture UIs can acknowledge the (queued) capture.
  MultimodalProcessingTaskInDB _localQueuedTask({
    required MultimodalInputType inputType,
    String? dataUrl,
    String? rawText,
    required Map<String, dynamic> metadata,
  }) {
    final now = DateTime.now();
    return MultimodalProcessingTaskInDB(
      id: 'local-${now.microsecondsSinceEpoch}',
      userId: 'self',
      inputType: inputType,
      inputUrl: dataUrl,
      inputRawText: rawText,
      metadata: {...metadata, 'local_pending_sync': true},
      createdAt: now,
      updatedAt: now,
    );
  }

  // --- Multimodal Processing Task Endpoints ---

  Future<MultimodalProcessingTaskInDB> createTask(MultimodalProcessingTaskCreate task) async {
    final response = await _client.post(
      Uri.parse('$_baseUrl/tasks/'),
      headers: await _getHeaders(),
      body: json.encode(task.toJson()),
    );

    if (response.statusCode == 201) {
      return MultimodalProcessingTaskInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to create multimodal task: ${response.body}');
    }
  }

  Future<MultimodalProcessingTaskInDB> getTask(String taskId) async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/tasks/$taskId'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      return MultimodalProcessingTaskInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to get multimodal task: ${response.body}');
    }
  }

  Future<List<MultimodalProcessingTaskInDB>> getAllTasks() async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/tasks/'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      Iterable l = json.decode(response.body);
      return List<MultimodalProcessingTaskInDB>.from(l.map((model) => MultimodalProcessingTaskInDB.fromJson(model)));
    } else {
      throw Exception('Failed to get all multimodal tasks: ${response.body}');
    }
  }

  Future<MultimodalProcessingTaskInDB> updateTask(String taskId, MultimodalProcessingTaskUpdate task) async {
    final response = await _client.put(
      Uri.parse('$_baseUrl/tasks/$taskId'),
      headers: await _getHeaders(),
      body: json.encode(task.toJson()),
    );

    if (response.statusCode == 200) {
      return MultimodalProcessingTaskInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to update multimodal task: ${response.body}');
    }
  }

  Future<void> deleteTask(String taskId) async {
    final response = await _client.delete(
      Uri.parse('$_baseUrl/tasks/$taskId'),
      headers: await _getHeaders(),
    );

    if (response.statusCode != 204) {
      throw Exception('Failed to delete multimodal task: ${response.body}');
    }
  }

  // --- User Correction Endpoints ---

  Future<UserCorrectionInDB> submitUserCorrection(String taskId, UserCorrection correction) async {
    final response = await _client.post(
      Uri.parse('$_baseUrl/tasks/$taskId/corrections'),
      headers: await _getHeaders(),
      body: json.encode(correction.toJson()),
    );

    if (response.statusCode == 201) {
      return UserCorrectionInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to submit user correction: ${response.body}');
    }
  }

  // --- Book Inbox (incomplete records wired to each Book) ---

  /// Lists the current Book's capture records.
  /// view: incomplete (default) | organized | all
  Future<List<MultimodalProcessingTaskInDB>> getBookInbox(
      {String view = 'incomplete', int limit = 100}) async {
    final cacheKey = 'inbox_${view}_$limit';
    try {
      final response = await _client.get(
        Uri.parse('$_baseUrl/inbox?view=$view&limit=$limit'),
        headers: await _getHeaders(),
      );

      if (response.statusCode == 200) {
        final records = List<MultimodalProcessingTaskInDB>.from(
            (json.decode(response.body) as Iterable)
                .map((model) => MultimodalProcessingTaskInDB.fromJson(model)));
        // Cache the last-known payload so the inbox stays readable offline
        // (potentially forever until the device reconnects).
        await _localDb.putOfflineCache(cacheKey, response.body);
        return records;
      } else {
        throw Exception('Failed to load Book inbox: ${response.body}');
      }
    } on OfflineException {
      final cached = await _localDb.getOfflineCache(cacheKey);
      if (cached != null) {
        return List<MultimodalProcessingTaskInDB>.from(
            (json.decode(cached) as Iterable)
                .map((model) => MultimodalProcessingTaskInDB.fromJson(model)));
      }
      return const [];
    }
  }

  /// Per-status counts for the Book inbox (badge-friendly).
  Future<Map<String, dynamic>> getInboxSummary() async {
    const cacheKey = 'inbox_summary';
    try {
      final response = await _client.get(
        Uri.parse('$_baseUrl/inbox/summary'),
        headers: await _getHeaders(),
      );

      if (response.statusCode == 200) {
        await _localDb.putOfflineCache(cacheKey, response.body);
        return json.decode(response.body) as Map<String, dynamic>;
      } else {
        throw Exception('Failed to load inbox summary: ${response.body}');
      }
    } on OfflineException {
      final cached = await _localDb.getOfflineCache(cacheKey);
      if (cached != null) return json.decode(cached) as Map<String, dynamic>;
      return {};
    }
  }

  /// Accepts an AI-organized record into the Book (marks completed).
  Future<MultimodalProcessingTaskInDB> organizeRecord(String taskId) async {
    final response = await _client.post(
      Uri.parse('$_baseUrl/inbox/$taskId/organize'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      return MultimodalProcessingTaskInDB.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to organize record: ${response.body}');
    }
  }

  /// Fire-and-forget photo capture: Vimbai processes it in the background.
  Future<MultimodalProcessingTaskInDB> capturePhoto(File imageFile,
      {String? sourceContext}) async {
    final dataUrl = await _fileToDataUrl(imageFile);
    final metadata = {'source_context': sourceContext ?? 'camera_photo'};
    try {
      return await createTask(MultimodalProcessingTaskCreate(
        userId: 'self',
        inputType: MultimodalInputType.image,
        inputUrl: dataUrl,
        metadata: metadata,
      ));
    } on OfflineException {
      // Offline: keep the capture on-device; SyncService will replay it.
      await _queueOfflineCapture(
          inputType: MultimodalInputType.image, dataUrl: dataUrl, metadata: metadata);
      return _localQueuedTask(
          inputType: MultimodalInputType.image, dataUrl: dataUrl, metadata: metadata);
    }
  }

  /// Fire-and-forget video capture.
  Future<MultimodalProcessingTaskInDB> captureVideo(File videoFile,
      {String? sourceContext}) async {
    final dataUrl = await _fileToDataUrl(videoFile);
    final metadata = {'source_context': sourceContext ?? 'camera_video'};
    try {
      return await createTask(MultimodalProcessingTaskCreate(
        userId: 'self',
        inputType: MultimodalInputType.video,
        inputUrl: dataUrl,
        metadata: metadata,
      ));
    } on OfflineException {
      await _queueOfflineCapture(
          inputType: MultimodalInputType.video, dataUrl: dataUrl, metadata: metadata);
      return _localQueuedTask(
          inputType: MultimodalInputType.video, dataUrl: dataUrl, metadata: metadata);
    }
  }

  /// Fire-and-forget typed/pasted note.
  Future<MultimodalProcessingTaskInDB> captureText(String text,
      {String? sourceContext}) async {
    final metadata = {'source_context': sourceContext ?? 'typed_note'};
    try {
      return await createTask(MultimodalProcessingTaskCreate(
        userId: 'self',
        inputType: MultimodalInputType.text,
        inputRawText: text,
        metadata: metadata,
      ));
    } on OfflineException {
      await _queueOfflineCapture(
          inputType: MultimodalInputType.text, rawText: text, metadata: metadata);
      return _localQueuedTask(
          inputType: MultimodalInputType.text, rawText: text, metadata: metadata);
    }
  }

  // --- User Correction Endpoints (existing) ---

  Future<List<UserCorrectionInDB>> getUserCorrections(String taskId) async {
    final response = await _client.get(
      Uri.parse('$_baseUrl/tasks/$taskId/corrections'),
      headers: await _getHeaders(),
    );

    if (response.statusCode == 200) {
      Iterable l = json.decode(response.body);
      return List<UserCorrectionInDB>.from(l.map((model) => UserCorrectionInDB.fromJson(model)));
    } else {
      throw Exception('Failed to get user corrections: ${response.body}');
    }
  }
}
