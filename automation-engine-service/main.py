from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from automation_engine_service import models, crud
from automation_engine_service.database import init_db_schema, Neo4jConnector
from automation_engine_service.dependencies import get_db_session, get_user_id # Assuming these are defined
from automation_engine_service.utils.auth import check_permission # Assuming check_permission is defined
from automation_engine_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Automation Engine Service",
    description="Manages and orchestrates automated tasks across FinAcc microservices.",
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
    await init_db_schema() # Initialize Neo4j schema specific to automation engine service

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

# --- Automation Task Definition Endpoints ---
@app.post("/task-definitions/", response_model=models.AutomationTaskDefinitionInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("automation.write.definitions"))])
async def create_automation_task_definition(
    definition: models.AutomationTaskDefinitionCreate,
    owner_user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    definition.owner_user_id = owner_user_id
    return await crud.create_automation_task_definition(db_session, definition)

@app.get("/task-definitions/", response_model=List[models.AutomationTaskDefinitionInDB],
             dependencies=[Depends(check_permission("automation.read.definitions"))])
async def read_all_automation_task_definitions(
    owner_user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_automation_task_definitions(db_session, owner_user_id)

@app.get("/task-definitions/{definition_id}", response_model=models.AutomationTaskDefinitionInDB,
             dependencies=[Depends(check_permission("automation.read.definitions"))])
async def read_automation_task_definition_by_id(
    definition_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    definition = await crud.get_automation_task_definition(db_session, definition_id)
    if definition is None:
        raise NotFoundError(detail="Automation Task Definition not found.")
    return definition

@app.put("/task-definitions/{definition_id}", response_model=models.AutomationTaskDefinitionInDB,
             dependencies=[Depends(check_permission("automation.write.definitions"))])
async def update_automation_task_definition(
    definition_id: str,
    definition: models.AutomationTaskDefinitionUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_definition = await crud.update_automation_task_definition(db_session, definition_id, definition)
    if updated_definition is None:
        raise NotFoundError(detail="Automation Task Definition not found.")
    return updated_definition

@app.delete("/task-definitions/{definition_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("automation.delete.definitions"))])
async def delete_automation_task_definition(
    definition_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_automation_task_definition(db_session, definition_id)
    if not success:
        raise NotFoundError(detail="Automation Task Definition not found.")
    return {"ok": True}

# --- Automation Task Instance Endpoints ---
@app.post("/task-instances/", response_model=models.AutomationTaskInstanceInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("automation.write.instances"))])
async def create_automation_task_instance(
    instance: models.AutomationTaskInstanceCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_automation_task_instance(db_session, instance)

@app.get("/task-instances/{instance_id}", response_model=models.AutomationTaskInstanceInDB,
             dependencies=[Depends(check_permission("automation.read.instances"))])
async def read_automation_task_instance_by_id(
    instance_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    instance = await crud.get_automation_task_instance(db_session, instance_id)
    if instance is None:
        raise NotFoundError(detail="Automation Task Instance not found.")
    return instance

@app.put("/task-instances/{instance_id}", response_model=models.AutomationTaskInstanceInDB,
             dependencies=[Depends(check_permission("automation.write.instances"))])
async def update_automation_task_instance(
    instance_id: str,
    instance: models.AutomationTaskInstanceUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_instance = await crud.update_automation_task_instance(db_session, instance_id, instance)
    if updated_instance is None:
        raise NotFoundError(detail="Automation Task Instance not found.")
    return updated_instance

# --- Automation Log Endpoints ---
@app.post("/task-instances/{instance_id}/logs/", response_model=models.AutomationLogInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("automation.write.logs"))])
async def create_automation_log(
    instance_id: str,
    log: models.AutomationLogCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    log.instance_id = instance_id
    return await crud.create_automation_log(db_session, log)

@app.get("/task-instances/{instance_id}/logs/", response_model=List[models.AutomationLogInDB],
             dependencies=[Depends(check_permission("automation.read.logs"))])
async def read_automation_logs_for_instance(
    instance_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_automation_logs_for_instance(db_session, instance_id)

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Automation Engine Service is running!"}
