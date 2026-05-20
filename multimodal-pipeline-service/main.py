from fastapi import FastAPI, Depends, HTTPException, status, UploadFile, File
from typing import List, Optional, Dict, Any, Union
from multimodal_pipeline_service.models import MultimodalInput, DocumentParseResult, AudioParseResult
from multimodal_pipeline_service.pipeline import MultimodalPipeline
from multimodal_pipeline_service.utils.auth import check_permission
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Multimodal Pipeline Service",
    description="Processes various input types (image, audio, text) for financial data extraction.",
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
    source_context: Optional[str] = None
):
    if not file.content_type or not file.content_type.startswith("image/"):
        raise HTTPException(status_code=400, detail="Only image files are supported for OCR.")
    
    image_bytes = await file.read()
    return await multimodal_pipeline.process_document_ocr(image_bytes)

@app.post("/process-audio-to-text",
              response_model=AudioParseResult,
              dependencies=[Depends(check_permission("multimodal.process.audio"))])
async def process_audio_for_transcription(
    file: UploadFile = File(...),
    source_context: Optional[str] = None
):
    if not file.content_type or not file.content_type.startswith("audio/"):
        raise HTTPException(status_code=400, detail="Only audio files are supported for transcription.")
    
    audio_bytes = await file.read()
    return await multimodal_pipeline.process_audio_to_text(audio_bytes)

@app.post("/process-multimodal-input",
              response_model=Union[DocumentParseResult, AudioParseResult, Dict[str, Any]],
              dependencies=[Depends(check_permission("multimodal.process.any"))])
async def process_general_multimodal_input(input_data: MultimodalInput):
    return await multimodal_pipeline.process_multimodal_input(input_data)

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Multimodal Pipeline Service is running!"}
