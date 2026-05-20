import 'dart:io';
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';
import 'package:record/record.dart';
import 'package:finacc_mobile_client/services/multimodal_api_service.dart';
import 'package:finacc_mobile_client/models/multimodal_models.dart';

class MultimodalInputPage extends StatefulWidget {
  const MultimodalInputPage({super.key});

  @override
  State<MultimodalInputPage> createState() => _MultimodalInputPageState();
}

class _MultimodalInputPageState extends State<MultimodalInputPage> {
  final MultimodalApiService _apiService = MultimodalApiService();
  final ImagePicker _picker = ImagePicker();
  final Record _audioRecorder = Record();

  String? _pickedImagePath;
  String? _recordedAudioPath;
  String? _sourceContext;

  bool _isProcessing = false;
  dynamic _results;
  String? _errorMessage;

  bool _isRecording = false;

  Future<void> _pickImage(ImageSource source) async {
    final pickedFile = await _picker.pickImage(source: source);
    if (pickedFile != null) {
      setState(() {
        _pickedImagePath = pickedFile.path;
        _recordedAudioPath = null;
        _results = null;
        _errorMessage = null;
      });
    }
  }

  Future<void> _startRecording() async {
    try {
      if (await _audioRecorder.hasPermission()) {
        await _audioRecorder.start(
          path: '${(await _getAppDirectory())}/audio.m4a', // Using a temporary path
          encoder: AudioEncoder.aacLc, // or other suitable encoder
        );
        setState(() {
          _isRecording = true;
          _pickedImagePath = null;
          _recordedAudioPath = null;
          _results = null;
          _errorMessage = null;
        });
      }
    } catch (e) {
      setState(() {
        _errorMessage = 'Error starting recording: $e';
      });
    }
  }

  Future<void> _stopRecording() async {
    final path = await _audioRecorder.stop();
    if (path != null) {
      setState(() {
        _recordedAudioPath = path;
        _isRecording = false;
      });
    }
  }

  Future<String> _getAppDirectory() async {
    // In a real app, use path_provider package for platform-specific paths
    // For now, a simplified approach
    return Directory.systemTemp.path; // temporary directory
  }

  Future<void> _processInput() async {
    setState(() {
      _isProcessing = true;
      _results = null;
      _errorMessage = null;
    });

    try {
      if (_pickedImagePath != null) {
        final result = await _apiService.processDocumentOcr(
          imageFile: File(_pickedImagePath!),
          sourceContext: _sourceContext,
        );
        setState(() { _results = result; });
      } else if (_recordedAudioPath != null) {
        final result = await _apiService.processAudioToText(
          audioFile: File(_recordedAudioPath!),
          sourceContext: _sourceContext,
        );
        setState(() { _results = result; });
      } else {
        _errorMessage = 'No image or audio selected/recorded.';
      }
    } catch (e) {
      setState(() { _errorMessage = 'Processing failed: $e'; });
    }
  } finally {
      setState(() { _isProcessing = false; });
    }
  }

  @override
  void dispose() {
    _audioRecorder.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Multimodal Input'),
      ),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text('Choose Input Type:', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
            const SizedBox(height: 10),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceAround,
              children: [
                ElevatedButton.icon(
                  onPressed: () => _pickImage(ImageSource.gallery),
                  icon: const Icon(Icons.photo_library),
                  label: const Text('Pick Image'),
                ),
                ElevatedButton.icon(
                  onPressed: () => _pickImage(ImageSource.camera),
                  icon: const Icon(Icons.camera_alt),
                  label: const Text('Take Photo'),
                ),
              ],
            ),
            const SizedBox(height: 20),
            Center(
              child: _pickedImagePath == null
                  ? const Text('No image selected.')
                  : Image.file(File(_pickedImagePath!), height: 200),
            ),
            const SizedBox(height: 20),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceAround,
              children: [
                ElevatedButton.icon(
                  onPressed: _isRecording ? _stopRecording : _startRecording,
                  icon: Icon(_isRecording ? Icons.stop : Icons.mic),
                  label: Text(_isRecording ? 'Stop Recording' : 'Start Recording'),
                  style: ElevatedButton.styleFrom(
                    backgroundColor: _isRecording ? Colors.red : Colors.blue,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 10),
            Center(
              child: _recordedAudioPath == null
                  ? const Text('No audio recorded.')
                  : Text('Audio recorded: ${_recordedAudioPath!.split('/').last}'),
            ),
            const SizedBox(height: 20),
            TextField(
              onChanged: (value) => _sourceContext = value,
              decoration: const InputDecoration(
                labelText: 'Source Context (e.g., "Receipt from coffee shop")',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 20),
            _isProcessing
                ? const Center(child: CircularProgressIndicator())
                : ElevatedButton(
                    onPressed: (_pickedImagePath != null || _recordedAudioPath != null) ? _processInput : null,
                    child: const Text('Process Input'),
                  ),
            const SizedBox(height: 20),
            if (_errorMessage != null)
              Text('Error: $_errorMessage', style: const TextStyle(color: Colors.red)),
            if (_results != null)
              _buildResultsDisplay(_results),
          ],
        ),
      ),
    );
  }

  Widget _buildResultsDisplay(dynamic results) {
    if (results is DocumentParseResult) {
      return Card(
        child: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Document Type: ${results.documentType}', style: const TextStyle(fontWeight: FontWeight.bold)),
              Text('Status: ${results.status} (Time: ${results.processingTimeMs ?? 'N/A'}ms)'),
              const Divider(),
              const Text('Extracted Data:', style: TextStyle(fontWeight: FontWeight.bold)),
              ...results.extractedData.map((field) => Text('${field.fieldName}: ${field.value} (Conf: ${field.confidence?.toStringAsFixed(2)}) ')),
              if (results.rawText != null) ...[
                const Divider(),
                const Text('Raw Text:', style: TextStyle(fontWeight: FontWeight.bold)),
                Text(results.rawText!),
              ],
              if (results.errorMessage != null) Text('Error: ${results.errorMessage!}', style: const TextStyle(color: Colors.red)),
            ],
          ),
        ),
      );
    } else if (results is AudioParseResult) {
      return Card(
        child: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('Transcription:', style: TextStyle(fontWeight: FontWeight.bold)),
              Text(results.transcription),
              const Divider(),
              if (results.extractedEntities != null && results.extractedEntities!.isNotEmpty) ...[
                const Text('Extracted Entities:', style: TextStyle(fontWeight: FontWeight.bold)),
                ...results.extractedEntities!.map((field) => Text('${field.fieldName}: ${field.value} (Conf: ${field.confidence?.toStringAsFixed(2)}) ')),
              ],
              Text('Status: ${results.status} (Time: ${results.processingTimeMs ?? 'N/A'}ms)'),
              if (results.errorMessage != null) Text('Error: ${results.errorMessage!}', style: const TextStyle(color: Colors.red)),
            ],
          ),
        ),
      );
    }
    return const SizedBox.shrink();
  }
}
