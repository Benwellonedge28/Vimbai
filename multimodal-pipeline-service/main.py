import uuid # NEW
from fastapi import FastAPI, Depends, HTTPException, status, UploadFile, File, Form, BackgroundTasks # NEW: BackgroundTasks
from fastapi.responses import JSONResponse
from typing import List, Optional, Dict, Any, Union
from multimodal_pipeline_service.models import (
    MultimodalInput, DocumentParseResult, AudioParseResult,
    AutomatedJournalEntryResponse, JournalEntryCreate, TaskStatusResponse # NEW
)
from multimodal_pipeline_service.processor import MultimodalProcessor, task_results # NEW: Use processor, task_results
from multimodal_pipeline_service.messaging import RabbitMQProducer, RabbitMQConsumer # NEW
from multimodal_pipeline_service.utils.auth import check_permission
from multimodal_pipeline_service.dependencies import get_jwt_token
from multimodal_pipeline_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv
import asyncio # NEW
import base64 # NEW

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Multimodal Pipeline Service",
    description="Processes various input types (image, audio, text) for financial data extraction and automation.",
    version="0.1.0",
)

processor = MultimodalProcessor() # Renamed pipeline to processor
rabbitmq_producer = RabbitMQProducer() # NEW
multimodal_queue = "multimodal_processing_queue" # NEW

# Worker function to consume messages from RabbitMQ
async def process_message_from_queue(message_data: Dict[str, Any]):
    task_id = message_data.get("task_id")
    multimodal_input_dict = message_data.get("multimodal_input")
    jwt_token = message_data.get("jwt_token")
    action = message_data.get("action") # "process_input" or "create_journal_entry"

    if not task_id or not multimodal_input_dict or not jwt_token or not action:
        print(f"Invalid message received: {message_data}")
        return
    
    multimodal_input = MultimodalInput(**multimodal_input_dict)
    task_results[task_id] = {"status": "processing", "progress": 0, "message": "Task started."}

    try:
        if action == "process_input":
            result = await processor.process_multimodal_input_sync(multimodal_input, jwt_token)
        elif action == "create_journal_entry":
            result = await processor.process_multimodal_to_journal_entry_sync(multimodal_input, jwt_token)
        else:
            raise ValueError(f"Unknown action: {action}")
        
        task_results[task_id] = {"status": "completed", "progress": 100, "result": result.model_dump_json() if hasattr(result, 'model_dump_json') else result}

    except Exception as e:
        print(f"Error processing async task {task_id}: {e}")
        task_results[task_id] = {"status": "failed", "progress": 100, "error": str(e)}

# Function to run the consumer in the background
def run_consumer():
    consumer = RabbitMQConsumer(multimodal_queue, process_message_from_queue)
    try:
        consumer.connect()
        consumer.start_consuming()
    except KeyboardInterrupt:
        consumer.stop_consuming()
    except Exception as e:
        print(f"RabbitMQ consumer error: {e}")
        consumer.stop_consuming()

@app.on_event("startup")
async def startup_event():
    await rabbitmq_producer.connect() # Connect producer
    # Start consumer in a separate thread/process or worker service
    # For simplicity in this single FastAPI app, we'll start it in a background task
    # In a real microservice deployment, this would be a separate worker process/service
    loop = asyncio.get_event_loop()
    loop.run_in_executor(None, run_consumer) # This runs the blocking consumer in a thread pool executor
    print("Multimodal Pipeline Service started.")

@app.on_event("shutdown")
async def shutdown_event():
    rabbitmq_producer.close()
    print("Multimodal Pipeline Service shutting down.")

# --- Global Exception Handlers ---
# ... (unchanged) ...

# --- Multimodal Processing Endpoints ---

@app.post("/process-document-ocr", 
              response_model=TaskStatusResponse, # MODIFIED to return TaskStatusResponse
              dependencies=[Depends(check_permission("multimodal.process.ocr"))])
async def process_document_for_ocr(
    background_tasks: BackgroundTasks, # NEW
    file: UploadFile = File(...),
    source_context: Optional[str] = Form(None),
    jwt_token: str = Depends(get_jwt_token) # Need JWT for async processing too
):
    if not file.content_type or not file.content_type.startswith("image/"):
        raise ValidationError(detail="Only image files are supported for OCR.", code="UNSUPPORTED_MEDIA_TYPE")
    
    image_bytes = await file.read()
    task_id = str(uuid.uuid4())
    # Encode image_bytes to base64 string for JSON transmission
    multimodal_input = MultimodalInput(input_type="base64_image", data=base64.b64encode(image_bytes).decode('utf-8'), source_context=source_context)
    
    # Publish task to RabbitMQ
    rabbitmq_producer.publish(multimodal_queue, {
        "task_id": task_id,
        "action": "process_input",
        "multimodal_input": multimodal_input.model_dump(),
        "jwt_token": jwt_token # Pass JWT for worker
    })
    task_results[task_id] = {"status": "queued", "progress": 0, "message": "Task queued for processing.", "result": None, "error": None}
    return TaskStatusResponse(task_id=task_id, status="queued", message="Processing initiated asynchronously.")

@app.post("/process-audio-to-text",
              response_model=TaskStatusResponse, # MODIFIED to return TaskStatusResponse
              dependencies=[Depends(check_permission("multimodal.process.audio"))])
async def process_audio_for_transcription(
    background_tasks: BackgroundTasks, # NEW
    file: UploadFile = File(...),
    source_context: Optional[str] = Form(None),
    jwt_token: str = Depends(get_jwt_token) # Need JWT for async processing too
):
    if not file.content_type or not file.content_type.startswith("audio/"):
        raise ValidationError(detail="Only audio files are supported for transcription.", code="UNSUPPORTED_MEDIA_TYPE")
    
    audio_bytes = await file.read()
    task_id = str(uuid.uuid4())
    # Encode audio_bytes to base64 string for JSON transmission
    multimodal_input = MultimodalInput(input_type="base64_audio", data=base64.b64encode(audio_bytes).decode('utf-8'), source_context=source_context)
    
    # Publish task to RabbitMQ
    rabbitmq_producer.publish(multimodal_queue, {
        "task_id": task_id,
        "action": "process_input",
        "multimodal_input": multimodal_input.model_dump(),
        "jwt_token": jwt_token
    })
    task_results[task_id] = {"status": "queued", "progress": 0, "message": "Task queued for processing.", "result": None, "error": None}
    return TaskStatusResponse(task_id=task_id, status="queued", message="Processing initiated asynchronously.")
    
@app.post("/process-multimodal-input",
              response_model=TaskStatusResponse, # MODIFIED to return TaskStatusResponse
              dependencies=[Depends(check_permission("multimodal.process.any"))])
async def process_general_multimodal_input(
    multimodal_input: MultimodalInput,
    jwt_token: str = Depends(get_jwt_token)
):
    task_id = str(uuid.uuid4())
    # Publish task to RabbitMQ
    rabbitmq_producer.publish(multimodal_queue, {
        "task_id": task_id,
        "action": "process_input",
        "multimodal_input": multimodal_input.model_dump(),
        "jwt_token": jwt_token
    })
    task_results[task_id] = {"status": "queued", "progress": 0, "message": "Task queued for processing.", "result": None, "error": None}
    return TaskStatusResponse(task_id=task_id, status="queued", message="Processing initiated asynchronously.")


@app.post("/multimodal-to-journal-entry",
              response_model=TaskStatusResponse, # MODIFIED to return TaskStatusResponse
              dependencies=[Depends(check_permission("multimodal.create.journal_entry"))])
async def create_journal_entry_from_multimodal(
    multimodal_input: MultimodalInput,
    jwt_token: str = Depends(get_jwt_token)
):
    task_id = str(uuid.uuid4())
    # Publish task to RabbitMQ
    rabbitmq_producer.publish(multimodal_queue, {
        "task_id": task_id,
        "action": "create_journal_entry",
        "multimodal_input": multimodal_input.model_dump(),
        "jwt_token": jwt_token
    })
    task_results[task_id] = {"status": "queued", "progress": 0, "message": "Journal Entry creation queued asynchronously.", "result": None, "error": None}
    return TaskStatusResponse(task_id=task_id, status="queued", message="Journal Entry creation initiated asynchronously.")

@app.get("/tasks/{task_id}/status", response_model=TaskStatusResponse) # NEW endpoint
async def get_task_status(task_id: str):
    status_info = task_results.get(task_id)
    if status_info is None:
        raise NotFoundError(detail="Task ID not found.", code="TASK_NOT_FOUND")
    return TaskStatusResponse(task_id=task_id, **status_info)


# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Multimodal Pipeline Service is running!"}
