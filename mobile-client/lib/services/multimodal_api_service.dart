// mobile-client/lib/services/multimodal_api_service.dart

import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:vimbai_mobile_client/models/multimodal_models.dart'; // Ensure these models exist or are defined
import 'package:vimbai_mobile_client/services/auth_service.dart'; // For getting authentication token

class MultimodalApiService {
  final String _baseUrl = 'http://localhost:8001'; // Replace with actual Multimodal Service URL
  final AuthService _authService = AuthService();

  Future<Map<String, String>> _getHeaders() async {
    final token = await _authService.getToken();
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer $token',
    };
  }

  // --- Multimodal Processing Task Endpoints ---

  Future<MultimodalProcessingTaskInDB> createTask(MultimodalProcessingTaskCreate task) async {
    final response = await http.post(
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
    final response = await http.get(
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
    final response = await http.get(
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
    final response = await http.put(
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
    final response = await http.delete(
      Uri.parse('$_baseUrl/tasks/$taskId'),
      headers: await _getHeaders(),
    );

    if (response.statusCode != 204) {
      throw Exception('Failed to delete multimodal task: ${response.body}');
    }
  }

  // --- User Correction Endpoints ---

  Future<UserCorrectionInDB> submitUserCorrection(String taskId, UserCorrection correction) async {
    final response = await http.post(
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

  Future<List<UserCorrectionInDB>> getUserCorrections(String taskId) async {
    final response = await http.get(
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
