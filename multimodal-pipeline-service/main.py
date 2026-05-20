from fastapi import FastAPI, Depends, HTTPException, status, UploadFile, File, Form
from typing import List, Optional, Dict, Any, Union
from multimodal_pipeline_service.models import (
    MultimodalInput, DocumentParseResult, AudioParseResult,
    AutomatedJournalEntryResponse, JournalEntryCreate
)
from multimodal_pipeline_service.pipeline import MultimodalPipeline
from multimodal_pipeline_service.utils.auth import check_permission
from multimodal_pipeline_service.dependencies import get_jwt_token # NEW
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Multimodal Pipeline Service",
    description="Processes various input types (image, audio, text) for financial data extraction and automation.",
    version="0.1.0",
)

multimodal_pipeline = MultimodalPipeline()

@app.on_event("startup")
async def startup_event():
    print("Multimodal Pipeline Service started.")

@app.on_event("shutdown")
async def shutdown_event():
    print("Multimodal Pipeline Service shutting down.")

# --- Multimodal Processing Endpoints ---

@app.post("/process-document-ocr", 
              response_model=DocumentParseResult, 
              dependencies=[Depends(check_permission("multimodal.process.ocr"))])
async def process_document_for_ocr(
    file: UploadFile = File(...),
    source_context: Optional[str] = Form(None)
):
    if not file.content_type or not file.content_type.startswith("image/"):
        raise HTTPException(status_code=400, detail="Only image files are supported for OCR.")
    
    image_bytes = await file.read()
    return await multimodal_pipeline.process_document_ocr(image_bytes, source_context)

@app.post("/process-audio-to-text",
              response_model=AudioParseResult,
              dependencies=[Depends(check_permission("multimodal.process.audio"))])
async def process_audio_for_transcription(
    file: UploadFile = File(...),
    source_context: Optional[str] = Form(None)
):
    if not file.content_type or not file.content_type.startswith("audio/"):
        raise HTTPException(status_code=400, detail="Only audio files are supported for transcription.")
    
    audio_bytes = await file.read()
    return await multimodal_pipeline.process_audio_to_text(audio_bytes, source_context)

@app.post("/process-multimodal-input",
              response_model=Union[DocumentParseResult, AudioParseResult, Dict[str, Any]],
              dependencies=[Depends(check_permission("multimodal.process.any"))])
async def process_general_multimodal_input(multimodal_input: MultimodalInput, jwt_token: str = Depends(get_jwt_token)):
    # Pass jwt_token to pipeline for potential internal service calls later
    return await multimodal_pipeline.process_multimodal_input(multimodal_input, jwt_token)

@app.post("/multimodal-to-journal-entry",
              response_model=AutomatedJournalEntryResponse,
              dependencies=[Depends(check_permission("multimodal.create.journal_entry"))])
async def create_journal_entry_from_multimodal(
    multimodal_input: MultimodalInput,
    jwt_token: str = Depends(get_jwt_token)
):
    # 1. Process the multimodal input to extract data
    processed_result = await multimodal_pipeline.process_multimodal_input(multimodal_input, jwt_token)
    
    extracted_data = []
    if isinstance(processed_result, DocumentParseResult):
        extracted_data = processed_result.extracted_data
    elif isinstance(processed_result, AudioParseResult):
        extracted_data = processed_result.extractedEntities if processed_result.extractedEntities else []
    elif isinstance(processed_result, dict) and processed_result.get("proposed_journal_entry"):
        # If text input was processed and already proposed a JE
        proposed_je_dict = processed_result["proposed_journal_entry"]
        if proposed_je_dict:
            proposed_je = JournalEntryCreate(**proposed_je_dict)
            return await multimodal_pipeline.send_journal_entry_to_accounting_service(jwt_token, proposed_je)
        else:
            return AutomatedJournalEntryResponse(
                status="failed",
                message="Text input processed but no valid journal entry could be proposed.",
                extracted_data=processed_result
            )

    if not extracted_data:
        return AutomatedJournalEntryResponse(
            status="failed",
            message="No financial data could be extracted from the multimodal input.",
            extracted_data=processed_result # Include full processed result for debugging
        )

    # 2. Map extracted data to a JournalEntryCreate model
    proposed_journal_entry = await multimodal_pipeline._map_extracted_data_to_journal_entry(
        extracted_data, multimodal_input.source_context
    )

    if not proposed_journal_entry:
        return AutomatedJournalEntryResponse(
            status="failed",
            message="Extracted data could not be mapped to a valid Journal Entry (e.g., missing amount, unbalanced).",
            extracted_data=processed_result # Include full processed result for debugging
        )

    # 3. Send the proposed Journal Entry to the Accounting Service
    return await multimodal_pipeline.send_journal_entry_to_accounting_service(jwt_token, proposed_journal_entry)

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Multimodal Pipeline Service is running!"}
