from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from workflow_service import models, crud
from workflow_service.database import init_db_schema, Neo4jConnector
from workflow_service.dependencies import get_db_session # Assuming get_db_session is defined
from workflow_service.utils.auth import check_permission # Assuming check_permission is defined
from workflow_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError # Assuming custom exceptions are defined
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Workflow Service",
    description="Manages workflow definitions, orchestrates workflow instances, and handles approvals.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j")
    )
    Neo4jConnector.get_driver()
    await init_db_schema() # Initialize Neo4j schema specific to workflow service

@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()

# --- Global Exception Handlers (placeholders) ---
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

# --- Workflow Definition Endpoints ---
@app.post("/workflow-definitions/", response_model=models.WorkflowDefinitionInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("workflow.write.definitions"))])
async def create_workflow_definition(
    definition: models.WorkflowDefinitionCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_workflow_definition(db_session, definition)

@app.get("/workflow-definitions/", response_model=List[models.WorkflowDefinitionInDB],
             dependencies=[Depends(check_permission("workflow.read.definitions"))])
async def read_all_workflow_definitions(
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_workflow_definitions(db_session)

@app.get("/workflow-definitions/{definition_id}", response_model=models.WorkflowDefinitionInDB,
             dependencies=[Depends(check_permission("workflow.read.definitions"))])
async def read_workflow_definition_by_id(
    definition_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    definition = await crud.get_workflow_definition(db_session, definition_id)
    if definition is None:
        raise NotFoundError(detail="Workflow Definition not found.")
    return definition

@app.put("/workflow-definitions/{definition_id}", response_model=models.WorkflowDefinitionInDB,
             dependencies=[Depends(check_permission("workflow.write.definitions"))])
async def update_workflow_definition(
    definition_id: str,
    definition: models.WorkflowDefinitionUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_definition = await crud.update_workflow_definition(db_session, definition_id, definition)
    if updated_definition is None:
        raise NotFoundError(detail="Workflow Definition not found.")
    return updated_definition

@app.delete("/workflow-definitions/{definition_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("workflow.delete.definitions"))])
async def delete_workflow_definition(
    definition_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_workflow_definition(db_session, definition_id)
    if not success:
        raise NotFoundError(detail="Workflow Definition not found.")
    return {"ok": True}

# --- Workflow Instance Endpoints ---
@app.post("/workflow-instances/", response_model=models.WorkflowInstanceInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("workflow.write.instances"))])
async def create_workflow_instance(
    instance: models.WorkflowInstanceCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_workflow_instance(db_session, instance)

@app.get("/workflow-instances/{instance_id}", response_model=models.WorkflowInstanceInDB,
             dependencies=[Depends(check_permission("workflow.read.instances"))])
async def read_workflow_instance_by_id(
    instance_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    instance = await crud.get_workflow_instance(db_session, instance_id)
    if instance is None:
        raise NotFoundError(detail="Workflow Instance not found.")
    return instance

@app.put("/workflow-instances/{instance_id}", response_model=models.WorkflowInstanceInDB,
             dependencies=[Depends(check_permission("workflow.write.instances"))])
async def update_workflow_instance(
    instance_id: str,
    instance: models.WorkflowInstanceUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_instance = await crud.update_workflow_instance(db_session, instance_id, instance)
    if updated_instance is None:
        raise NotFoundError(detail="Workflow Instance not found.")
    return updated_instance

@app.delete("/workflow-instances/{instance_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("workflow.delete.instances"))])
async def delete_workflow_instance(
    instance_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_workflow_instance(db_session, instance_id)
    if not success:
        raise NotFoundError(detail="Workflow Instance not found.")
    return {"ok": True}

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Workflow Service is running!"}
