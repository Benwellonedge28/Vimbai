from datetime import datetime
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field


# --- Generic Multimodal Input Model ---
class MultimodalInput(BaseModel):
    id: str = Field(..., description="Unique ID for the multimodal input task.")
    user_id: str = Field(..., description="ID of the user who provided the input.")
    input_type: Literal["document", "audio", "image", "text", "video"] = Field(
        ..., description="Type of the input data."
    )
    data_url: Optional[str] = Field(None, description="URL to the raw input data (e.g., image file, audio file).")
    raw_text: Optional[str] = Field(None, description="Raw text content if input_type is 'text' or from ASR/OCR.")
    status: Literal["pending", "processing", "review_required", "completed", "failed"] = Field("pending")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    metadata: Dict[str, Any] = Field({}, description="Additional metadata for the input.")
    # New fields for linking to processing task
    processing_task_id: Optional[str] = Field(None, description="ID of the associated MultimodalProcessingTask.")


# --- Extracted Data Field (generic for any type of extraction) ---
class ExtractedDataField(BaseModel):
    name: str = Field(..., description="Name of the extracted field (e.g., 'total_amount', 'vendor_name').")
    value: str = Field(..., description="Extracted value as a string.")
    confidence: Optional[float] = Field(
        None, ge=0.0, le=1.0, description="Confidence score of the extraction (0.0 to 1.0)."
    )
    data_type: Literal["string", "number", "date", "boolean", "currency"] = Field(
        "string", description="Inferred data type."
    )
    unit: Optional[str] = Field(None, description="Unit for the value (e.g., 'USD', '%').")
    bounding_box: Optional[List[float]] = Field(
        None, description="Bounding box coordinates [x1, y1, x2, y2] if applicable."
    )
    original_value: Optional[str] = Field(
        None, description="The original raw value from which this field was extracted."
    )


# --- Document Parsing Result Models (for OCR, PDF analysis etc.) ---
class DocumentParseResult(BaseModel):
    raw_text: Optional[str] = Field(None, description="Full extracted text from the document.")
    extracted_data: List[ExtractedDataField] = Field([], description="Structured data fields extracted.")
    ai_confidence: Optional[float] = Field(
        None, ge=0.0, le=1.0, description="Overall AI confidence for the extraction."
    )
    errors: List[str] = Field([], description="List of errors encountered during parsing.")


# --- Audio Parsing Result Models (for ASR) ---
class AudioParseResult(BaseModel):
    transcribed_text: Optional[str] = Field(None, description="Full transcribed text from the audio.")
    extracted_commands: List[ExtractedDataField] = Field(
        [], description="Structured commands or data extracted from audio."
    )
    ai_confidence: Optional[float] = Field(
        None, ge=0.0, le=1.0, description="Overall AI confidence for the transcription/extraction."
    )
    errors: List[str] = Field([], description="List of errors encountered during parsing.")


# --- Image Parsing Result Models (for object detection, scene understanding) ---
class ImageParseResult(BaseModel):
    description: Optional[str] = Field(None, description="AI-generated description of the image content.")
    extracted_objects: List[ExtractedDataField] = Field([], description="Detected objects and their attributes.")
    ai_confidence: Optional[float] = Field(
        None, ge=0.0, le=1.0, description="Overall AI confidence for the image analysis."
    )
    errors: List[str] = Field([], description="List of errors encountered during parsing.")


# --- Multimodal Processing Task (new core entity for this service) ---
class MultimodalProcessingTaskBase(BaseModel):
    user_id: str = Field(..., description="ID of the user who initiated the task.")
    input_type: Literal["document", "audio", "image", "text", "video"] = Field(
        ..., description="Type of the original input data."
    )
    input_url: Optional[str] = Field(None, description="URL to the original raw input data.")
    input_raw_text: Optional[str] = Field(
        None, description="Raw text content if directly provided or from initial OCR/ASR."
    )
    status: Literal[
        "received", "processing", "ai_extracted", "review_pending", "user_corrected", "completed", "failed"
    ] = Field("received")
    processing_start_time: Optional[datetime] = None
    processing_end_time: Optional[datetime] = None
    last_review_request_time: Optional[datetime] = None
    document_result: Optional[DocumentParseResult] = Field(None, description="Results from document parsing.")
    audio_result: Optional[AudioParseResult] = Field(None, description="Results from audio parsing.")
    image_result: Optional[ImageParseResult] = Field(None, description="Results from image parsing.")
    suggested_journal_entry: Optional[Dict[str, Any]] = Field(
        None, description="AI's suggestion for a journal entry structure."
    )
    linked_vimbai_entity_id: Optional[str] = Field(
        None, description="ID of the Vimbai entity (e.g., JournalEntry) created from this task."
    )
    errors: List[str] = Field([], description="List of errors during processing.")
    metadata: Dict[str, Any] = Field({}, description="Additional metadata for the processing task.")
    ai_model_version: str = Field("1.0", description="Version of the AI model used for extraction.")


class MultimodalProcessingTaskCreate(MultimodalProcessingTaskBase):
    pass


class MultimodalProcessingTaskUpdate(BaseModel):
    status: Optional[
        Literal["received", "processing", "ai_extracted", "review_pending", "user_corrected", "completed", "failed"]
    ] = None
    processing_start_time: Optional[datetime] = None
    processing_end_time: Optional[datetime] = None
    last_review_request_time: Optional[datetime] = None
    document_result: Optional[DocumentParseResult] = None
    audio_result: Optional[AudioParseResult] = None
    image_result: Optional[ImageParseResult] = None
    suggested_journal_entry: Optional[Dict[str, Any]] = None
    linked_vimbai_entity_id: Optional[str] = None
    errors: Optional[List[str]] = None
    metadata: Optional[Dict[str, Any]] = None
    ai_model_version: Optional[str] = None


class MultimodalProcessingTaskInDB(MultimodalProcessingTaskBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# --- User Correction / Feedback Model (for improving AI) ---
class UserCorrection(BaseModel):
    task_id: str = Field(..., description="ID of the MultimodalProcessingTask this correction relates to.")
    user_id: str = Field(..., description="ID of the user who provided the correction.")
    field_name: str = Field(..., description="Name of the field that was corrected (e.g., 'total_amount').")
    original_value: Optional[str] = Field(None, description="Value originally extracted by AI.")
    corrected_value: str = Field(..., description="Value provided by the user.")
    feedback_type: Literal["value_correction", "missing_field", "incorrect_category"] = Field(
        ..., description="Type of feedback."
    )
    comment: Optional[str] = Field(None, description="Additional comments from the user.")
    submitted_at: datetime = Field(default_factory=datetime.utcnow)


class UserCorrectionInDB(UserCorrection):
    id: str = Field(..., example="uuid-string-for-node")

    class Config:
        from_attributes = True
