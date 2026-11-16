// mobile-client/lib/models/multimodal_models.dart

import 'package:json_annotation/json_annotation.dart';

part 'multimodal_models.g.dart'; // Generated file for json_serializable

enum MultimodalInputType {
  @JsonValue('document')
  document,
  @JsonValue('audio')
  audio,
  @JsonValue('image')
  image,
  @JsonValue('text')
  text,
  @JsonValue('video')
  video,
}

enum MultimodalProcessingStatus {
  @JsonValue('received')
  received,
  @JsonValue('processing')
  processing,
  @JsonValue('ai_extracted')
  aiExtracted,
  @JsonValue('review_pending')
  reviewPending,
  @JsonValue('user_corrected')
  userCorrected,
  @JsonValue('completed')
  completed,
  @JsonValue('failed')
  failed,
}

// --- MultimodalInput (from the mobile client's perspective, representing raw input) ---
@JsonSerializable()
class MultimodalInput {
  final String id;
  final String userId;
  final MultimodalInputType inputType;
  final String? dataUrl;
  final String? rawText;
  final MultimodalProcessingStatus status;
  final DateTime createdAt;
  final DateTime updatedAt;
  final Map<String, dynamic> metadata;
  final String? processingTaskId;

  MultimodalInput({
    required this.id,
    required this.userId,
    required this.inputType,
    this.dataUrl,
    this.rawText,
    this.status = MultimodalProcessingStatus.received,
    required this.createdAt,
    required this.updatedAt,
    this.metadata = const {},
    this.processingTaskId,
  });

  factory MultimodalInput.fromJson(Map<String, dynamic> json) =>
      _$MultimodalInputFromJson(json);
  Map<String, dynamic> toJson() => _$MultimodalInputToJson(this);
}

// --- ExtractedDataField (generic for any type of extraction) ---
@JsonSerializable()
class ExtractedDataField {
  final String name;
  final String value;
  final double? confidence;
  final String
  dataType; // e.g., "string", "number", "date", "boolean", "currency"
  final String? unit;
  final List<double>? boundingBox; // [x1, y1, x2, y2]
  final String? originalValue;

  ExtractedDataField({
    required this.name,
    required this.value,
    this.confidence,
    this.dataType = 'string',
    this.unit,
    this.boundingBox,
    this.originalValue,
  });

  factory ExtractedDataField.fromJson(Map<String, dynamic> json) =>
      _$ExtractedDataFieldFromJson(json);
  Map<String, dynamic> toJson() => _$ExtractedDataFieldToJson(this);
}

// --- DocumentParseResult (for OCR, PDF analysis etc.) ---
@JsonSerializable()
class DocumentParseResult {
  final String? rawText;
  final List<ExtractedDataField> extractedData;
  final double? aiConfidence;
  final List<String> errors;

  DocumentParseResult({
    this.rawText,
    this.extractedData = const [],
    this.aiConfidence,
    this.errors = const [],
  });

  factory DocumentParseResult.fromJson(Map<String, dynamic> json) =>
      _$DocumentParseResultFromJson(json);
  Map<String, dynamic> toJson() => _$DocumentParseResultToJson(this);
}

// --- AudioParseResult (for ASR) ---
@JsonSerializable()
class AudioParseResult {
  final String? transcribedText;
  final List<ExtractedDataField> extractedCommands;
  final double? aiConfidence;
  final List<String> errors;

  AudioParseResult({
    this.transcribedText,
    this.extractedCommands = const [],
    this.aiConfidence,
    this.errors = const [],
  });

  factory AudioParseResult.fromJson(Map<String, dynamic> json) =>
      _$AudioParseResultFromJson(json);
  Map<String, dynamic> toJson() => _$AudioParseResultToJson(this);
}

// --- ImageParseResult (for object detection, scene understanding) ---
@JsonSerializable()
class ImageParseResult {
  final String? description;
  final List<ExtractedDataField> extractedObjects;
  final double? aiConfidence;
  final List<String> errors;

  ImageParseResult({
    this.description,
    this.extractedObjects = const [],
    this.aiConfidence,
    this.errors = const [],
  });

  factory ImageParseResult.fromJson(Map<String, dynamic> json) =>
      _$ImageParseResultFromJson(json);
  Map<String, dynamic> toJson() => _$ImageParseResultToJson(this);
}

// --- MultimodalProcessingTask (core entity for this service) ---
@JsonSerializable()
class MultimodalProcessingTaskBase {
  final String userId;
  final MultimodalInputType inputType;
  final String? inputUrl;
  final String? inputRawText;
  final MultimodalProcessingStatus status;
  final DateTime? processingStartTime;
  final DateTime? processingEndTime;
  final DateTime? lastReviewRequestTime;
  final DocumentParseResult? documentResult;
  final AudioParseResult? audioResult;
  final ImageParseResult? imageResult;
  final Map<String, dynamic>? suggestedJournalEntry;
  final String? linkedFinaccEntityId;
  final List<String> errors;
  final Map<String, dynamic> metadata;
  final String aiModelVersion;

  MultimodalProcessingTaskBase({
    required this.userId,
    required this.inputType,
    this.inputUrl,
    this.inputRawText,
    this.status = MultimodalProcessingStatus.received,
    this.processingStartTime,
    this.processingEndTime,
    this.lastReviewRequestTime,
    this.documentResult,
    this.audioResult,
    this.imageResult,
    this.suggestedJournalEntry,
    this.linkedFinaccEntityId,
    this.errors = const [],
    this.metadata = const {},
    this.aiModelVersion = "1.0",
  });

  Map<String, dynamic> toJson() => _$MultimodalProcessingTaskBaseToJson(this);
}

@JsonSerializable()
class MultimodalProcessingTaskCreate extends MultimodalProcessingTaskBase {
  MultimodalProcessingTaskCreate({
    required String userId,
    required MultimodalInputType inputType,
    String? inputUrl,
    String? inputRawText,
    MultimodalProcessingStatus status = MultimodalProcessingStatus.received,
    DateTime? processingStartTime,
    DateTime? processingEndTime,
    DateTime? lastReviewRequestTime,
    DocumentParseResult? documentResult,
    AudioParseResult? audioResult,
    ImageParseResult? imageResult,
    Map<String, dynamic>? suggestedJournalEntry,
    String? linkedFinaccEntityId,
    List<String> errors = const [],
    Map<String, dynamic> metadata = const {},
    String aiModelVersion = "1.0",
  }) : super(
         userId: userId,
         inputType: inputType,
         inputUrl: inputUrl,
         inputRawText: inputRawText,
         status: status,
         processingStartTime: processingStartTime,
         processingEndTime: processingEndTime,
         lastReviewRequestTime: lastReviewRequestTime,
         documentResult: documentResult,
         audioResult: audioResult,
         imageResult: imageResult,
         suggestedJournalEntry: suggestedJournalEntry,
         linkedFinaccEntityId: linkedFinaccEntityId,
         errors: errors,
         metadata: metadata,
         aiModelVersion: aiModelVersion,
       );

  factory MultimodalProcessingTaskCreate.fromJson(Map<String, dynamic> json) =>
      _$MultimodalProcessingTaskCreateFromJson(json);
  @override
  Map<String, dynamic> toJson() => _$MultimodalProcessingTaskCreateToJson(this);
}

@JsonSerializable()
class MultimodalProcessingTaskUpdate {
  final MultimodalProcessingStatus? status;
  final DateTime? processingStartTime;
  final DateTime? processingEndTime;
  final DateTime? lastReviewRequestTime;
  final DocumentParseResult? documentResult;
  final AudioParseResult? audioResult;
  final ImageParseResult? imageResult;
  final Map<String, dynamic>? suggestedJournalEntry;
  final String? linkedFinaccEntityId;
  final List<String>? errors;
  final Map<String, dynamic>? metadata;
  final String? aiModelVersion;

  MultimodalProcessingTaskUpdate({
    this.status,
    this.processingStartTime,
    this.processingEndTime,
    this.lastReviewRequestTime,
    this.documentResult,
    this.audioResult,
    this.imageResult,
    this.suggestedJournalEntry,
    this.linkedFinaccEntityId,
    this.errors,
    this.metadata,
    this.aiModelVersion,
  });

  factory MultimodalProcessingTaskUpdate.fromJson(Map<String, dynamic> json) =>
      _$MultimodalProcessingTaskUpdateFromJson(json);
  Map<String, dynamic> toJson() => _$MultimodalProcessingTaskUpdateToJson(this);
}

@JsonSerializable()
class MultimodalProcessingTaskInDB extends MultimodalProcessingTaskBase {
  final String id;
  final DateTime createdAt;
  final DateTime updatedAt;

  MultimodalProcessingTaskInDB({
    required this.id,
    required String userId,
    required MultimodalInputType inputType,
    String? inputUrl,
    String? inputRawText,
    MultimodalProcessingStatus status = MultimodalProcessingStatus.received,
    DateTime? processingStartTime,
    DateTime? processingEndTime,
    DateTime? lastReviewRequestTime,
    DocumentParseResult? documentResult,
    AudioParseResult? audioResult,
    ImageParseResult? imageResult,
    Map<String, dynamic>? suggestedJournalEntry,
    String? linkedFinaccEntityId,
    List<String> errors = const [],
    Map<String, dynamic> metadata = const {},
    String aiModelVersion = "1.0",
    required this.createdAt,
    required this.updatedAt,
  }) : super(
         userId: userId,
         inputType: inputType,
         inputUrl: inputUrl,
         inputRawText: inputRawText,
         status: status,
         processingStartTime: processingStartTime,
         processingEndTime: processingEndTime,
         lastReviewRequestTime: lastReviewRequestTime,
         documentResult: documentResult,
         audioResult: audioResult,
         imageResult: imageResult,
         suggestedJournalEntry: suggestedJournalEntry,
         linkedFinaccEntityId: linkedFinaccEntityId,
         errors: errors,
         metadata: metadata,
         aiModelVersion: aiModelVersion,
       );

  /// Reads a capture as the engine stores it.
  ///
  /// The engine keeps a capture under its own names (`input_type`, `input_raw_text`, `created_at`) and stamps no
  /// `userId`, because a capture belongs to the Book it came through rather than to a field on the row. The generated
  /// parser reads camelCase and casts `userId` non-null, so a capture straight from the engine threw instead of
  /// appearing in the inbox. This maps the engine's names onto the ones the generated parser reads and leaves a
  /// payload already written in camelCase alone, so a locally queued capture and an engine row both load.
  factory MultimodalProcessingTaskInDB.fromJson(Map<String, dynamic> json) =>
      _$MultimodalProcessingTaskInDBFromJson(engineCaptureNames(json));
  @override
  Map<String, dynamic> toJson() => _$MultimodalProcessingTaskInDBToJson(this);
}

/// The engine's capture names, rewritten as the camelCase the generated parser reads.
///
/// Only names the engine itself answers are touched, and a key already in camelCase is kept, so one factory loads both
/// an engine row and a capture queued on the phone.
/// The engine's name for each capture field, kept once so a read and a write cannot drift apart.
const captureNames = {
  'input_type': 'inputType',
  'input_url': 'inputUrl',
  'input_raw_text': 'inputRawText',
  'processing_start_time': 'processingStartTime',
  'processing_end_time': 'processingEndTime',
  'last_review_request_time': 'lastReviewRequestTime',
  'document_result': 'documentResult',
  'audio_result': 'audioResult',
  'image_result': 'imageResult',
  'suggested_journal_entry': 'suggestedJournalEntry',
  'linked_vimbai_entity_id': 'linkedFinaccEntityId',
  'ai_model_version': 'aiModelVersion',
  'created_at': 'createdAt',
  'updated_at': 'updatedAt',
};

Map<String, dynamic> engineCaptureNames(Map<String, dynamic> json) {
  final named = <String, dynamic>{};
  json.forEach((key, value) {
    named[captureNames[key] ?? key] = value;
  });
  // the engine stamps a time as epoch seconds, and the generated parser reads an ISO string, so a row straight from the
  // engine threw on `createdAt` after the names had been mapped
  for (final field in ['createdAt', 'updatedAt']) {
    if (named[field] != null) named[field] = isoMoment(named[field]);
  }
  // a capture belongs to the Book it came through, so the engine stamps no user of its own
  named['userId'] = named['userId'] ?? '';
  final version = named['aiModelVersion'];
  if (version is! String || version.isEmpty) {
    named['aiModelVersion'] = '1.0';
  }
  return named;
}

/// A capture written the way the engine reads one.
///
/// The engine takes a capture by its own names and refuses one whose `input_type` it cannot read, so a capture sent in the
/// app's camelCase was refused outright: the field arrived as `inputType`, the engine saw no kind of input, and the note
/// never reached the inbox. Nulls are dropped rather than sent, because the engine stamps its own nulls and a null means
/// the same as absent.
Map<String, dynamic> engineCaptureRequest(Map<String, dynamic> written) {
  final reversed = {for (final e in captureNames.entries) e.value: e.key};
  final request = <String, dynamic>{};
  written.forEach((key, value) {
    if (value == null) return;
    // a capture belongs to the Book it came through; the engine does not read a user off the row
    if (key == 'userId') return;
    request[reversed[key] ?? key] = value;
  });
  return request;
}

// --- User Correction / Feedback Model ---
enum UserCorrectionType {
  @JsonValue('value_correction')
  valueCorrection,
  @JsonValue('missing_field')
  missingField,
  @JsonValue('incorrect_category')
  incorrectCategory,
}

@JsonSerializable()
class UserCorrection {
  final String taskId;
  final String userId;
  final String fieldName;
  final String? originalValue;
  final String correctedValue;
  final UserCorrectionType feedbackType;
  final String? comment;
  final DateTime submittedAt;

  UserCorrection({
    required this.taskId,
    required this.userId,
    required this.fieldName,
    this.originalValue,
    required this.correctedValue,
    required this.feedbackType,
    this.comment,
    required this.submittedAt,
  });

  factory UserCorrection.fromJson(Map<String, dynamic> json) =>
      _$UserCorrectionFromJson(json);
  Map<String, dynamic> toJson() => _$UserCorrectionToJson(this);
}

@JsonSerializable()
class UserCorrectionInDB extends UserCorrection {
  final String id;

  UserCorrectionInDB({
    required this.id,
    required String taskId,
    required String userId,
    required String fieldName,
    String? originalValue,
    required String correctedValue,
    required UserCorrectionType feedbackType,
    String? comment,
    required DateTime submittedAt,
  }) : super(
         taskId: taskId,
         userId: userId,
         fieldName: fieldName,
         originalValue: originalValue,
         correctedValue: correctedValue,
         feedbackType: feedbackType,
         comment: comment,
         submittedAt: submittedAt,
       );

  factory UserCorrectionInDB.fromJson(Map<String, dynamic> json) =>
      _$UserCorrectionInDBFromJson(engineCorrectionNames(json));
  @override
  Map<String, dynamic> toJson() => _$UserCorrectionInDBToJson(this);
}

/// A moment the way the generated parsers read one.
///
/// The engine stamps a time as epoch seconds (`1763300000.5`), sometimes inside a string; a payload already written as an
/// ISO string is left alone. Without this, every engine row threw on its first date field.
String isoMoment(Object? stamped) {
  if (stamped is num) {
    return DateTime.fromMillisecondsSinceEpoch(
      (stamped * 1000).round(),
    ).toIso8601String();
  }
  if (stamped is String) {
    final seconds = double.tryParse(stamped);
    if (seconds != null) {
      return DateTime.fromMillisecondsSinceEpoch(
        (seconds * 1000).round(),
      ).toIso8601String();
    }
    return stamped;
  }
  return DateTime.fromMillisecondsSinceEpoch(0).toIso8601String();
}

/// A correction as the engine stores it, rewritten as the camelCase the generated parser reads.
///
/// The engine keeps a correction as `task_id`, `field_name`, `original_value`, `corrected_value`, `reason` and
/// `created_at`, and stamps no user and no kind of correction - a person overruled a reading, which is the same act
/// whatever it was called. Reading it as it comes keeps a correction made on the phone visible afterwards instead of
/// throwing on the way back.
Map<String, dynamic> engineCorrectionNames(Map<String, dynamic> json) {
  const renamed = {
    'task_id': 'taskId',
    'field_name': 'fieldName',
    'original_value': 'originalValue',
    'corrected_value': 'correctedValue',
    'reason': 'comment',
    'created_at': 'submittedAt',
  };
  final named = <String, dynamic>{};
  json.forEach((key, value) {
    named[renamed[key] ?? key] = value;
  });
  named['taskId'] = named['taskId'] ?? '';
  named['userId'] = named['userId'] ?? '';
  named['fieldName'] = named['fieldName'] ?? '';
  // the engine keeps the value as it was written, which need not be a string
  for (final field in ['originalValue', 'correctedValue', 'comment']) {
    final held = named[field];
    if (held != null && held is! String) named[field] = held.toString();
  }
  named['correctedValue'] = named['correctedValue'] ?? '';
  named['feedbackType'] = named['feedbackType'] ?? 'value_correction';
  named['submittedAt'] = isoMoment(named['submittedAt']);
  return named;
}

/// A correction written the way the engine reads one.
///
/// The app's own name for a correction (`feedbackType`, `comment`) is kept in the app; what the engine records is the
/// field touched, the value it started from, the value it became and why. Sending the app's names left the field empty and
/// the value null, so a correction was kept and told nobody what had been corrected.
Map<String, dynamic> engineCorrectionRequest(UserCorrection correction) => {
  'task_id': correction.taskId,
  'field_name': correction.fieldName,
  'original_value': correction.originalValue,
  'corrected_value': correction.correctedValue,
  'reason': correction.comment ?? '',
  'feedback_type': _$UserCorrectionTypeEnumMap[correction.feedbackType],
};
