from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from fraud_detection_service import models, crud
from fraud_detection_service.database import init_db_schema, Neo4jConnector
from fraud_detection_service.dependencies import get_db_session
from fraud_detection_service.utils.auth import check_permission
from fraud_detection_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
from fraud_detection_service.ml_model import FraudDetector
import os
from dotenv import load_dotenv
from pydantic import ValidationError as PydanticValidationError

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Fraud Detection Service",
    description="Detects and flags suspicious financial transactions using ML models.",
    version="0.1.0",
)

# Initialize ML Model
fraud_detector = FraudDetector()

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j")
    )
    Neo4jConnector.get_driver()
    await init_db_schema()

@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()

# --- Global Exception Handlers ---
@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request, exc: NotFoundError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(ConflictError)
async def conflict_exception_handler(request, exc: ConflictError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(ValidationError)
async def validation_exception_handler(request, exc: ValidationError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )
    
@app.exception_handler(UnauthorizedError)
async def unauthorized_exception_handler(request, exc: UnauthorizedError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
        headers={"WWW-Authenticate": "Bearer"},
    )

@app.exception_handler(ForbiddenError)
async def forbidden_exception_handler(request, exc: ForbiddenError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(PydanticValidationError)
async def pydantic_validation_exception_handler(request, exc: PydanticValidationError):
    errors = exc.errors()
    error_details = []
    for error in errors:
        loc = ".".join(map(str, error["loc"]))
        error_details.append(f"Field '{loc}': {error["msg"]}")
    return JSONResponse(
        status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
        content={"detail": "Validation error: " + "; ".join(error_details), "code": "PYDANTIC_VALIDATION_ERROR"},
    )

# --- Fraud Detection Endpoints ---
@app.post("/analyze-transaction/", response_model=models.FraudDetectionResult,
          dependencies=[Depends(check_permission("fraud_detection.analyze_transaction"))])
async def analyze_transaction_for_fraud(
    transaction: models.TransactionForFraudCheck,
    db_session: AsyncSession = Depends(get_db_session)
):
    detection_result = fraud_detector.predict_fraud(transaction)
    
    if detection_result.fraud_score >= 0.5: # Flag suspicious or high-risk transactions
        fraud_flag_create = models.FraudulentTransactionFlagCreate(
            transaction_id=detection_result.transaction_id,
            fraud_score=detection_result.fraud_score,
            fraud_flag=detection_result.fraud_flag,
            reason=detection_result.reason,
            model_version=detection_result.model_version
        )
        await crud.create_fraud_flag(db_session, fraud_flag_create)
    
    return detection_result

@app.get("/fraud-flags/", response_model=List[models.FraudulentTransactionFlagInDB],
         dependencies=[Depends(check_permission("fraud_detection.read_flags"))])
async def get_all_fraud_flags(db_session: AsyncSession = Depends(get_db_session)):
    return await crud.get_all_fraud_flags(db_session)

@app.get("/fraud-flags/{flag_id}", response_model=models.FraudulentTransactionFlagInDB,
         dependencies=[Depends(check_permission("fraud_detection.read_flags"))])
async def get_fraud_flag_by_id(flag_id: str, db_session: AsyncSession = Depends(get_db_session)):n    db_flag = await crud.get_fraud_flag(db_session, flag_id)
    if db_flag is None:
        raise NotFoundError(detail="Fraud flag not found.", code="FRAUD_FLAG_NOT_FOUND")
    return db_flag

@app.put("/fraud-flags/{flag_id}/status", response_model=models.FraudulentTransactionFlagInDB,
         dependencies=[Depends(check_permission("fraud_detection.manage_flags"))])
async def update_fraud_flag_status(
    flag_id: str,
    status_update: Literal["open", "investigating", "false_positive", "confirmed_fraud"],
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_flag = await crud.update_fraud_flag_status(db_session, flag_id, status_update)
    if updated_flag is None:
        raise NotFoundError(detail="Fraud flag not found.", code="FRAUD_FLAG_NOT_FOUND")
    return updated_flag

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Fraud Detection Service is running!"}
