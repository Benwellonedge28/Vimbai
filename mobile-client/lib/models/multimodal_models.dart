// mobile-client/lib/models/multimodal_models.dart

import 'package:json_annotation/json_annotation.dart';
import 'package:flutter/foundation.dart'; // For @required if not null safety
import 'package:uuid/uuid.dart'; // For generating UUIDs if needed

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

  factory MultimodalInput.fromJson(Map<String, dynamic> json) => _$MultimodalInputFromJson(json);
  Map<String, dynamic> toJson() => _$MultimodalInputToJson(this);
}


// --- ExtractedDataField (generic for any type of extraction) ---
@JsonSerializable()
class ExtractedDataField {
  final String name;
  final String value;
  final double? confidence;
  final String dataType; // e.g., "string", "number", "date", "boolean", "currency"
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

  factory ExtractedDataField.fromJson(Map<String, dynamic> json) => _$ExtractedDataFieldFromJson(json);
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

  factory DocumentParseResult.fromJson(Map<String, dynamic> json) => _$DocumentParseResultFromJson(json);
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

  factory AudioParseResult.fromJson(Map<String, dynamic> json) => _$AudioParseResultFromJson(json);
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

  factory ImageParseResult.fromJson(Map<String, dynamic> json) => _$ImageParseResultFromJson(json);
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

  factory MultimodalProcessingTaskCreate.fromJson(Map<String, dynamic> json) => _$MultimodalProcessingTaskCreateFromJson(json);
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

  factory MultimodalProcessingTaskUpdate.fromJson(Map<String, dynamic> json) => _$MultimodalProcessingTaskUpdateFromJson(json);
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

  factory MultimodalProcessingTaskInDB.fromJson(Map<String, dynamic> json) => _$MultimodalProcessingTaskInDBFromJson(json);
  @override
  Map<String, dynamic> toJson() => _$MultimodalProcessingTaskInDBToJson(this);
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

  factory UserCorrection.fromJson(Map<String, dynamic> json) => _$UserCorrectionFromJson(json);
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

  factory UserCorrectionInDB.fromJson(Map<String, dynamic> json) => _$UserCorrectionInDBFromJson(json);
  @override
  Map<String, dynamic> toJson() => _$UserCorrectionInDBToJson(this);
}
