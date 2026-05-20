class ExtractedField {
  final String fieldName;
  final String value;
  final double? confidence;
  final List<double>? boundingBox;

  ExtractedField({
    required this.fieldName,
    required this.value,
    this.confidence,
    this.boundingBox,
  });

  factory ExtractedField.fromJson(Map<String, dynamic> json) {
    return ExtractedField(
      fieldName: json['field_name'],
      value: json['value'],
      confidence: (json['confidence'] as num?)?.toDouble(),
      boundingBox: (json['bounding_box'] as List<dynamic>?)?.map((e) => (e as num).toDouble()).toList(),
    );
  }
}

class DocumentParseResult {
  final String documentType;
  final List<ExtractedField> extractedData;
  final String? rawText;
  final String status;
  final int? processingTimeMs;
  final String? errorMessage;

  DocumentParseResult({
    required this.documentType,
    required this.extractedData,
    this.rawText,
    required this.status,
    this.processingTimeMs,
    this.errorMessage,
  });

  factory DocumentParseResult.fromJson(Map<String, dynamic> json) {
    var extractedDataFromJson = json['extracted_data'] as List;
    List<ExtractedField> extractedDataList = extractedDataFromJson.map((itemJson) => ExtractedField.fromJson(itemJson)).toList();

    return DocumentParseResult(
      documentType: json['document_type'],
      extractedData: extractedDataList,
      rawText: json['raw_text'],
      status: json['status'],
      processingTimeMs: json['processing_time_ms'],
      errorMessage: json['error_message'],
    );
  }
}

class AudioParseResult {
  final String transcription;
  final List<dynamic>? speakerDiarization; // Using dynamic as structure is not fully defined
  final List<ExtractedField>? extractedEntities;
  final String status;
  final int? processingTimeMs;
  final String? errorMessage;

  AudioParseResult({
    required this.transcription,
    this.speakerDiarization,
    this.extractedEntities,
    required this.status,
    this.processingTimeMs,
    this.errorMessage,
  });

  factory AudioParseResult.fromJson(Map<String, dynamic> json) {
    List<ExtractedField>? entitiesList;
    if (json['extracted_entities'] != null) {
      var extractedEntitiesFromJson = json['extracted_entities'] as List;
      entitiesList = extractedEntitiesFromJson.map((itemJson) => ExtractedField.fromJson(itemJson)).toList();
    }

    return AudioParseResult(
      transcription: json['transcription'],
      speakerDiarization: json['speaker_diarization'] as List<dynamic>?,
      extractedEntities: entitiesList,
      status: json['status'],
      processingTimeMs: json['processing_time_ms'],
      errorMessage: json['error_message'],
    );
  }
}
