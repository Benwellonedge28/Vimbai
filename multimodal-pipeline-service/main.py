import asyncio
import os
from datetime import datetime
from typing import List, Optional

from dotenv import load_dotenv
from fastapi import BackgroundTasks, Depends, FastAPI, HTTPException, Request, status
from fastapi.responses import JSONResponse
from multimodal_pipeline_service import crud, models
from multimodal_pipeline_service.database import Neo4jConnector, init_db_schema
from multimodal_pipeline_service.dependencies import get_db_session, get_user_id
from multimodal_pipeline_service.exceptions import (
    ConflictError,
    ForbiddenError,
    NotFoundError,
    UnauthorizedError,
    ValidationError,
)
from multimodal_pipeline_service.services.ai_processor import AIProcessor  # NEW: Will define this soon
from multimodal_pipeline_service.utils.auth import check_permission
from neo4j import AsyncSession

# Load environment variables
load_dotenv()

app = FastAPI(
    title="Vimbai Multimodal Pipeline Service",
    description="Processes multimodal inputs (documents, audio, images) to extract financial data.",
    version="0.1.0",
)


# Distributed tracing
try:
    from shared.tracing import get_tracer, setup_tracing

    TRACER = setup_tracing(service_name="multimodal-pipeline-service", instrument_app=app)
except ImportError:
    TRACER = None
    import logging

    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")


@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j"),
    )
    Neo4jConnector.get_driver()
    await init_db_schema()


@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()


# --- Global Exception Handlers ---
@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)


@app.exception_handler(ConflictError)
async def conflict_exception_handler(request, exc: ConflictError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)


@app.exception_handler(ValidationError)
async def validation_exception_handler(request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)


@app.exception_handler(UnauthorizedError)
async def unauthorized_exception_handler(request, exc: UnauthorizedError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail, headers={"WWW-Authenticate": "Bearer"})


@app.exception_handler(ForbiddenError)
async def forbidden_exception_handler(request, exc: ForbiddenError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)


# --- Multimodal Processing Task Endpoints (NEW) ---
@app.post(
    "/tasks/",
    response_model=models.MultimodalProcessingTaskInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("multimodal.write.tasks"))],
)
async def create_multimodal_task(
    task_data: models.MultimodalProcessingTaskCreate,
    background_tasks: BackgroundTasks,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    task_data.user_id = user_id
    task = await crud.create_multimodal_processing_task(db_session, task_data)

    # Trigger AI processing in the background
    background_tasks.add_task(AIProcessor(db_session).process_multimodal_task, task.id)
    # Alternatively, send to Automation Engine for scheduling/retries:
    # from automation_engine_service.clients.automation_engine_client import AutomationEngineClient
    # ae_client = AutomationEngineClient()
    # ae_client.create_task_instance(task_definition_id="multimodal_process_task_def", context={"task_id": task.id})

    return task


@app.get(
    "/tasks/",
    response_model=List[models.MultimodalProcessingTaskInDB],
    dependencies=[Depends(check_permission("multimodal.read.tasks"))],
)
async def get_all_multimodal_tasks(
    user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_multimodal_processing_tasks(db_session, user_id)


@app.get(
    "/tasks/{task_id}",
    response_model=models.MultimodalProcessingTaskInDB,
    dependencies=[Depends(check_permission("multimodal.read.tasks"))],
)
async def get_multimodal_task_by_id(task_id: str, db_session: AsyncSession = Depends(get_db_session)):
    task = await crud.get_multimodal_processing_task(db_session, task_id)
    if task is None:
        raise NotFoundError(detail="Multimodal Processing Task not found.")
    return task


@app.put(
    "/tasks/{task_id}",
    response_model=models.MultimodalProcessingTaskInDB,
    dependencies=[Depends(check_permission("multimodal.write.tasks"))],
)
async def update_multimodal_task(
    task_id: str, task_update: models.MultimodalProcessingTaskUpdate, db_session: AsyncSession = Depends(get_db_session)
):
    updated_task = await crud.update_multimodal_processing_task(db_session, task_id, task_update)
    if updated_task is None:
        raise NotFoundError(detail="Multimodal Processing Task not found.")
    return updated_task


@app.delete(
    "/tasks/{task_id}",
    status_code=status.HTTP_204_NO_CONTENT,
    dependencies=[Depends(check_permission("multimodal.delete.tasks"))],
)
async def delete_multimodal_task(task_id: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_multimodal_processing_task(db_session, task_id)
    if not success:
        raise NotFoundError(detail="Multimodal Processing Task not found.")
    return {"ok": True}


# --- User Correction / Feedback Endpoints (NEW) ---
@app.post(
    "/tasks/{task_id}/corrections",
    response_model=models.UserCorrectionInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("multimodal.write.corrections"))],
)
async def submit_user_correction(
    task_id: str,
    correction_data: models.UserCorrection,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    # Ensure the task exists and belongs to the user
    task = await crud.get_multimodal_processing_task(db_session, task_id)
    if not task or task.user_id != user_id:
        raise NotFoundError(detail="Multimodal Processing Task not found or not owned by user.")

    correction_data.task_id = task_id
    correction_data.user_id = user_id
    correction = await crud.create_user_correction(db_session, correction_data)

    # Update the task status to reflect user correction and potentially re-process/re-evaluate
    await crud.update_multimodal_processing_task(
        db_session,
        task_id,
        models.MultimodalProcessingTaskUpdate(
            status="user_corrected", updated_at=datetime.utcnow()  # Ensure updated_at is set
        ),
    )

    # In a real scenario, this would trigger model retraining or fine-tuning based on the feedback.
    print(
        f"User correction received for task {task_id}: {correction_data.field_name} from {correction_data.original_value} to {correction_data.corrected_value}"
    )

    return correction


@app.get(
    "/tasks/{task_id}/corrections",
    response_model=List[models.UserCorrectionInDB],
    dependencies=[Depends(check_permission("multimodal.read.corrections"))],
)
async def get_user_corrections(
    task_id: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    task = await crud.get_multimodal_processing_task(db_session, task_id)
    if not task or task.user_id != user_id:
        raise NotFoundError(detail="Multimodal Processing Task not found or not owned by user.")

    return await crud.get_user_corrections_for_task(db_session, task_id)


# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "Vimbai Multimodal Pipeline Service is running!"}
