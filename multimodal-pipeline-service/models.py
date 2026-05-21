from pydantic import BaseModel, Field, condecimal, validator
from typing import Optional, List, Literal, Dict, Any, Union
from datetime import datetime
from decimal import Decimal

# --- Existing Multimodal Models (unchanged) ---
class ExtractedField(BaseModel):
    field_name: str = Field(..., example="total_amount", description="Name of the extracted field.")
    value: str = Field(..., example="123.45", description="Extracted value as a string.")
    confidence: Optional[float] = Field(None, ge=0.0, le=1.0, description="Confidence score of the extraction.")
    bounding_box: Optional[List[float]] = Field(None, description="[x_min, y_min, x_max, y_max] coordinates.")

class DocumentParseResult(BaseModel):
    document_type: str = Field(..., example="receipt", description="Categorized type of the document.")
    extracted_data: List[ExtractedField] = Field(..., description="List of extracted fields.")
    raw_text: Optional[str] = Field(None, description="Full OCR text from the document.")
    status: str = Field(..., example="completed", description="Status of the parsing operation.")
    processing_time_ms: Optional[int] = Field(None, description="Time taken for processing in milliseconds.")
    error_message: Optional[str] = Field(None, description="Error message if processing failed.")

class AudioParseResult(BaseModel):
    transcription: str = Field(..., description="Full transcription of the audio.")
    speaker_diarization: Optional[List[Dict[str, Any]]] = Field(None, description="Speaker segmentation details.")
    extracted_entities: Optional[List[ExtractedField]] = Field(None, description="Key entities extracted from transcription.")
    status: str = Field(..., example="completed", description="Status of the parsing operation.")
    processing_time_ms: Optional[int] = Field(None, description="Time taken for processing in milliseconds.")
    error_message: Optional[str] = Field(None, description="Error message if processing failed.")

class MultimodalInput(BaseModel):
    input_type: str = Field(..., example="image_url", description="Type of input (e.g., 'image_url', 'base64_image', 'audio_url', 'text').")
    data: str = Field(..., description="The input data (URL, Base64 string, text content).")
    source_context: Optional[str] = Field(None, description="Additional context about the source of the input.")

# --- Journal Entry Models (copied from accounting-service for inter-service communication) ---
class JournalLineBase(BaseModel):
    account_number: str
    debit: Decimal = Field(Decimal('0.00'))
    credit: Decimal = Field(Decimal('0.00'))
    description: Optional[str] = None

class JournalEntryCreate(BaseModel):
    entry_date: datetime = Field(default_factory=datetime.utcnow)
    description: str
    reference_number: Optional[str] = None
    source_module: str = "Multimodal"
    lines: List[JournalLineBase]
    status: Literal['pending', 'posted', 'reviewed', 'voided'] = Field('pending', description="Current status of the journal entry.")

# --- Fraud Detection Service Models (copied from fraud-detection-service for inter-service communication) ---
class TransactionForFraudCheck(BaseModel):
    transaction_id: str = Field(..., description="Unique ID of the transaction.")
    amount: condecimal(decimal_places=2, gt=Decimal('0.00')) = Field(..., description="Amount of the transaction.")
    currency: str = Field("USD", max_length=3, description="Currency of the transaction (ISO 4217).")
    sender_account_id: str = Field(..., description="ID of the sender's account.")
    recipient_account_id: str = Field(..., description="ID of the recipient's account.")
    transaction_type: Literal["debit", "credit", "transfer", "payment", "purchase"] = Field(..., description="Type of transaction.")
    timestamp: datetime = Field(default_factory=datetime.utcnow, description="Timestamp of the transaction.")
    location_data: Optional[Dict[str, Any]] = Field(None, description="Geographic location data of the transaction.")
    device_info: Optional[Dict[str, Any]] = Field(None, description="Device information (e.g., IP address, OS, browser).")
    previous_transactions_count_24h: int = Field(0, ge=0, description="Number of transactions by sender in last 24h.")
    avg_daily_transaction_amount_7d: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Average daily transaction amount by sender over 7 days.")

    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class FraudDetectionResult(BaseModel):
    transaction_id: str = Field(..., description="ID of the transaction that was analyzed.")
    fraud_score: float = Field(..., ge=0.0, le=1.0, description="Probability or score indicating likelihood of fraud (0-1).")
    fraud_flag: Literal["safe", "low_risk", "suspicious", "high_risk"] = Field(..., description="Categorical flag for fraud risk.")
    reason: Optional[str] = Field(None, description="Reason or rules triggered for the flag.")
    model_version: str = Field(..., description="Version of the ML model used for detection.")

class AutomatedJournalEntryResponse(BaseModel):
    status: str = Field(..., example="success", description="Status of the journal entry creation.")
    message: str = Field(..., example="Journal entry created successfully.", description="Detailed message.")
    journal_entry_id: Optional[str] = Field(None, description="ID of the created journal entry.")
    extracted_data: Optional[Dict[str, Any]] = Field(None, description="Data extracted from multimodal input.")
    proposed_journal_entry: Optional[JournalEntryCreate] = Field(None, description="The journal entry proposed/created.")
    fraud_detection_result: Optional[FraudDetectionResult] = Field(None, description="Result from fraud detection service.") # NEW


# --- Task Status Response (unchanged) ---
class TaskStatusResponse(BaseModel):
    task_id: str
    status: Literal["queued", "processing", "completed", "failed"]
    progress: int = Field(0, ge=0, le=100)
    message: Optional[str] = None
    result: Optional[Union[DocumentParseResult, AudioParseResult, AutomatedJournalEntryResponse, Dict[str, Any]]] = None # Store serialized result
    error: Optional[str] = None

# --- Error Response Model (unchanged) ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
