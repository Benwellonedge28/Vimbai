import 'dart:convert';
import 'dart:io';
import 'package:http/http.dart' as http;
import 'package:http_parser/http_parser.dart';
import 'package:finacc_mobile_client/local_db/user_local_data.dart';
import 'package:finacc_mobile_client/config.dart'; // For API URL
import 'package:finacc_mobile_client/models/multimodal_models.dart'; // NEW: Import Multimodal Models

class MultimodalApiService {
  final String _multimodalServiceUrl = '${AppConfig.apiUrl.replaceFirst(':8080', ':8002')}'; // Hardcoded for now

  Future<Map<String, String>> _getAuthHeaders() async {
    final token = await UserLocalData.getAuthToken();
    return {
      'Authorization': 'Bearer $token',
    };
  }

  Future<DocumentParseResult> processDocumentOcr({
    required File imageFile,
    String? sourceContext,
  }) async {
    final headers = await _getAuthHeaders();
    var request = http.MultipartRequest(
      'POST',
      Uri.parse('$_multimodalServiceUrl/process-document-ocr'),
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
      return DocumentParseResult.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to process document OCR: ${response.body}');
    }
  }

  Future<AudioParseResult> processAudioToText({
    required File audioFile,
    String? sourceContext,
  }) async {
    final headers = await _getAuthHeaders();
    var request = http.MultipartRequest(
      'POST',
      Uri.parse('$_multimodalServiceUrl/process-audio-to-text'),
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
      return AudioParseResult.fromJson(json.decode(response.body));
    } else {
      throw Exception('Failed to process audio to text: ${response.body}');
    }
  }

  // Method for general multimodal input (e.g., URL, text)
  Future<dynamic> processMultimodalInput({
    required String inputType,
    required String data,
    String? sourceContext,
  }) async {
    final headers = await _getAuthHeaders();
    final body = json.encode({
      'input_type': inputType,
      'data': data,
      'source_context': sourceContext,
    });

    final response = await http.post(
      Uri.parse('$_multimodalServiceUrl/process-multimodal-input'),
      headers: {'Content-Type': 'application/json', ...headers},
      body: body,
    );

    if (response.statusCode == 200) {
      final jsonResponse = json.decode(response.body);
      if (inputType.contains('image')) {
        return DocumentParseResult.fromJson(jsonResponse);
      } else if (inputType.contains('audio')) {
        return AudioParseResult.fromJson(jsonResponse);
      }
      return jsonResponse; // For text or generic response
    } else {
      throw Exception('Failed to process multimodal input: ${response.body}');
    }
  }
}
