from pydantic import BaseModel, Field
from typing import Optional, List, Dict, Any
from datetime import datetime

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
# These models define the structure expected by the Accounting Service's create_journal_entry endpoint.
from decimal import Decimal # Use Decimal for financial calculations

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

# Response model for automated Journal Entry creation
class AutomatedJournalEntryResponse(BaseModel):
    status: str = Field(..., example="success", description="Status of the journal entry creation.")
    message: str = Field(..., example="Journal entry created successfully.", description="Detailed message.")
    journal_entry_id: Optional[str] = Field(None, description="ID of the created journal entry.")
    extracted_data: Optional[Dict[str, Any]] = Field(None, description="Data extracted from multimodal input.")
    proposed_journal_entry: Optional[JournalEntryCreate] = Field(None, description="The journal entry proposed/created.")
