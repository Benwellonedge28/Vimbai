import 'dart:convert';
import 'dart:io';
import 'package:http/http.dart' as http;
import 'package:http_parser/http_parser.dart';
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/multimodal_models.dart'; // Import Multimodal Models
import 'package:finacc_mobile_client/local_db/database_helper.dart'; // NEW: For local DB access
import 'package:connectivity_plus/connectivity_plus.dart'; // NEW: For offline detection
import 'package:uuid/uuid.dart'; // NEW: For generating UUIDs

class MultimodalApiService {
  final String _processDocumentOcrUrl = '${AppConfig.apiUrl}/process-document-ocr';
  final String _processAudioToTextUrl = '${AppConfig.apiUrl}/process-audio-to-text';
  final String _processMultimodalInputUrl = '${AppConfig.apiUrl}/process-multimodal-input';

  final DatabaseHelper _dbHelper = DatabaseHelper(); // NEW

  Future<Map<String, String>> _getAuthHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Authorization': 'Bearer $token',
    };
  }

  // Helper to convert File to base64
  Future<String> _fileToBase64(File file) async {
    List<int> imageBytes = await file.readAsBytes();
    return base64Encode(imageBytes);
  }

  Future<DocumentParseResult> processDocumentOcr({
    required File imageFile,
    String? sourceContext,
    bool isOffline = false, // NEW: isOffline param
  }) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    final String base64Image = await _fileToBase64(imageFile);
    final multimodalInput = MultimodalInput(
      inputType: 'base64_image',
      data: base64Image,
      sourceContext: sourceContext,
    );

    if (isOffline || connectivityResult == ConnectivityResult.none) {
      // Store task locally if offline
      await _dbHelper.insertMultimodalTask(multimodalInput, isSynced: false);
      print('Offline: Stored document OCR task locally.');
      return DocumentParseResult(
        rawText: 'Offline: Task queued locally.',
        extractedData: [],
        status: 'queued_offline',
        error_message: 'No internet connection, task queued for later sync.',
        processing_time: 0,
      );
    }

    // Try to send to remote
    try {
      final headers = await _getAuthHeaders();
      var request = http.MultipartRequest(
        'POST',
        Uri.parse(_processDocumentOcrUrl),
      );
      request.headers.addAll(headers);
      request.files.add(await http.MultipartFile.fromPath(
        'file',
        imageFile.path,
        contentType: MediaType('image', imageFile.path.split('.').last),
      ));
      if (sourceContext != null) {
        request.fields['source_context'] = sourceContext;
      }

      var streamedResponse = await request.send();
      var response = await http.Response.fromStream(streamedResponse);

      if (response.statusCode == 200) {
        print('Online: Successfully sent document OCR task remotely.');
        return DocumentParseResult.fromJson(json.decode(response.body));
      } else {
        print('Online: Failed to process document OCR remotely: ${response.statusCode} - ${response.body}');
        throw Exception('Failed to process document OCR: ${response.body}');
      }
    } catch (e) {
      print('Online: Error processing document OCR remotely, attempting local save: $e');
      // If remote fails, save it locally for later sync
      await _dbHelper.insertMultimodalTask(multimodalInput, isSynced: false);
      throw Exception('Failed to process document OCR remotely, queued offline: $e');
    }
  }

  Future<AudioParseResult> processAudioToText({
    required File audioFile,
    String? sourceContext,
    bool isOffline = false, // NEW: isOffline param
  }) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    final String base64Audio = await _fileToBase64(audioFile);
    final multimodalInput = MultimodalInput(
      inputType: 'base64_audio',
      data: base64Audio,
      sourceContext: sourceContext,
    );

    if (isOffline || connectivityResult == ConnectivityResult.none) {
      // Store task locally if offline
      await _dbHelper.insertMultimodalTask(multimodalInput, isSynced: false);
      print('Offline: Stored audio to text task locally.');
      return AudioParseResult(
        transcribedText: 'Offline: Task queued locally.',
        extractedCommands: [],
        status: 'queued_offline',
        error_message: 'No internet connection, task queued for later sync.',
        processing_time: 0,
      );
    }

    // Try to send to remote
    try {
      final headers = await _getAuthHeaders();
      var request = http.MultipartRequest(
        'POST',
        Uri.parse(_processAudioToTextUrl),
      );
      request.headers.addAll(headers);
      request.files.add(await http.MultipartFile.fromPath(
        'file',
        audioFile.path,
        contentType: MediaType('audio', audioFile.path.split('.').last),
      ));
      if (sourceContext != null) {
        request.fields['source_context'] = sourceContext;
      }

      var streamedResponse = await request.send();
      var response = await http.Response.fromStream(streamedResponse);

      if (response.statusCode == 200) {
        print('Online: Successfully sent audio to text task remotely.');
        return AudioParseResult.fromJson(json.decode(response.body));
      } else {
        print('Online: Failed to process audio to text remotely: ${response.statusCode} - ${response.body}');
        throw Exception('Failed to process audio to text: ${response.body}');
      }
    } catch (e) {
      print('Online: Error processing audio to text remotely, attempting local save: $e');
      // If remote fails, save it locally for later sync
      await _dbHelper.insertMultimodalTask(multimodalInput, isSynced: false);
      throw Exception('Failed to process audio to text remotely, queued offline: $e');
    }
  }

  // Method for general multimodal input (e.g., URL, text)
  Future<dynamic> processMultimodalInput({
    required String inputType,
    required String data,
    String? sourceContext,
    bool isOffline = false, // NEW: isOffline param
  }) async {
    final connectivityResult = await (Connectivity().checkConnectivity());
    final multimodalInput = MultimodalInput(
      inputType: inputType,
      data: data,
      sourceContext: sourceContext,
    );

    if (isOffline || connectivityResult == ConnectivityResult.none) {
      // Store task locally if offline
      await _dbHelper.insertMultimodalTask(multimodalInput, isSynced: false);
      print('Offline: Stored general multimodal input task locally.');
      // Return a dummy response or a queued status
      return {'status': 'queued_offline', 'message': 'No internet connection, task queued for later sync.'}; // Return Map
    }

    // Try to send to remote
    try {
      final headers = await _getAuthHeaders();
      final body = json.encode(multimodalInput.toJson()); // Use toJson() for MultimodalInput

      final response = await http.post(
        Uri.parse(_processMultimodalInputUrl),
        headers: {'Content-Type': 'application/json', ...headers},
        body: body,
      );

      if (response.statusCode == 200) {
        print('Online: Successfully sent general multimodal input task remotely.');
        final jsonResponse = json.decode(response.body);
        if (inputType.contains('image')) {
          return DocumentParseResult.fromJson(jsonResponse);
        } else if (inputType.contains('audio')) {
          return AudioParseResult.fromJson(jsonResponse);
        }
        return jsonResponse; // For text or generic response
      } else {
        print('Online: Failed to process general multimodal input remotely: ${response.statusCode} - ${response.body}');
        throw Exception('Failed to process multimodal input: ${response.body}');
      }
    } catch (e) {
      print('Online: Error processing general multimodal input remotely, attempting local save: $e');
      // If remote fails, save it locally for later sync
      await _dbHelper.insertMultimodalTask(multimodalInput, isSynced: false);
      throw Exception('Failed to process multimodal input remotely, queued offline: $e');
    }
  }

  // NEW: --- Sync Offline Multimodal Tasks ---
  Future<void> syncOfflineMultimodalTasks() async {
    print('Attempting to sync offline multimodal tasks...');
    final connectivityResult = await (Connectivity().checkConnectivity());
    if (connectivityResult == ConnectivityResult.none) {
      print('Sync failed: No internet connection.');
      throw Exception('No internet connection to sync data.');
    }

    final unsyncedTasks = await _dbHelper.getUnsyncedMultimodalTasks();
    if (unsyncedTasks.isEmpty) {
      print('No unsynced multimodal tasks found.');
      return;
    }

    print('Found ${unsyncedTasks.length} unsynced multimodal tasks. Starting sync...');
    for (var task in unsyncedTasks) {
      try {
        if (task.inputType == 'base64_image') {
          final bytes = base64Decode(task.data);
          final tempFile = await File('${(await Directory.systemTemp()).path}/temp_image_${const Uuid().v4()}.png').writeAsBytes(bytes);
          await processDocumentOcr(imageFile: tempFile, sourceContext: task.sourceContext, isOffline: false); // Force online processing
          await tempFile.delete(); // Clean up temp file
        } else if (task.inputType == 'base64_audio') {
          final bytes = base64Decode(task.data);
          final tempFile = await File('${(await Directory.systemTemp()).path}/temp_audio_${const Uuid().v4()}.mp3').writeAsBytes(bytes);
          await processAudioToText(audioFile: tempFile, sourceContext: task.sourceContext, isOffline: false); // Force online processing
          await tempFile.delete();
        } else {
          // For general multimodal input (e.g., URL, text)
          await processMultimodalInput(inputType: task.inputType, data: task.data, sourceContext: task.sourceContext, isOffline: false); // Force online processing
        }
        await _dbHelper.markMultimodalTaskAsSynced(task.id!); // Mark as synced
        print('Successfully synced offline multimodal task: ${task.inputType} - ${task.id}');
      } on http.ClientException catch (e) {
        print('Network error during sync for multimodal task ${task.id}: $e');
      } catch (e) {
        print('Failed to sync multimodal task ${task.id}: $e');
      }
    }
    print('Offline multimodal task sync complete.');
  }
}
