"""
Vimbai Encrypted Backup Service
Handles encrypted backup and restore operations for financial data.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "encrypted-backup-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8412"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Encrypted Backup Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class BackupJob(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    service_name: str
    backup_type: str  # full, incremental, differential
    status: str = "pending"  # pending, running, completed, failed
    file_path: str = ""
    encryption_key_id: str = ""
    size_bytes: int = 0
    checksum: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    completed_at: Optional[datetime] = None
    created_by: str = ""


class RestoreJob(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    backup_id: str
    status: str = "pending"  # pending, running, completed, failed
    restored_by: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    completed_at: Optional[datetime] = None


class CreateBackupRequest(BaseModel):
    service_name: str
    backup_type: str = "full"
    encryption_key_id: str = ""
    created_by: str = ""


backups: List[BackupJob] = []
restores: List[RestoreJob] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/backup", response_model=BackupJob)
async def create_backup(request: CreateBackupRequest):
    """Create an encrypted backup job."""
    valid_types = ["full", "incremental", "differential"]
    if request.backup_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid backup type. Must be one of {valid_types}")

    job = BackupJob(
        service_name=request.service_name,
        backup_type=request.backup_type,
        encryption_key_id=request.encryption_key_id or str(uuid.uuid4()),
        created_by=request.created_by,
        status="running",
    )

    # Simulate backup completion
    job.status = "completed"
    job.completed_at = datetime.now(timezone.utc)
    job.file_path = f"/backups/{job.id}.enc"
    job.size_bytes = 0
    job.checksum = f"sha256:{uuid.uuid4().hex}"

    backups.append(job)
    logger.info("Backup completed", backup_id=job.id, service=request.service_name, type=request.backup_type)
    return job


@app.get("/backups", response_model=List[BackupJob])
async def list_backups(service_name: Optional[str] = None, status: Optional[str] = None):
    """List backup jobs with optional filters."""
    result = backups
    if service_name:
        result = [b for b in result if b.service_name == service_name]
    if status:
        result = [b for b in result if b.status == status]
    return result


@app.get("/backups/{backup_id}", response_model=BackupJob)
async def get_backup(backup_id: str):
    """Get a specific backup job."""
    job = next((b for b in backups if b.id == backup_id), None)
    if not job:
        raise HTTPException(status_code=404, detail="Backup not found")
    return job


@app.post("/backup/{backup_id}/restore", response_model=RestoreJob)
async def restore_backup(backup_id: str, restored_by: str = ""):
    """Restore from a backup job."""
    backup = next((b for b in backups if b.id == backup_id), None)
    if not backup:
        raise HTTPException(status_code=404, detail="Backup not found")
    if backup.status != "completed":
        raise HTTPException(status_code=400, detail="Backup is not in completed state")

    restore = RestoreJob(
        backup_id=backup_id,
        restored_by=restored_by,
        status="running",
    )
    # Simulate restore completion
    restore.status = "completed"
    restore.completed_at = datetime.now(timezone.utc)
    restores.append(restore)
    logger.info("Restore completed", restore_id=restore.id, backup_id=backup_id)
    return restore


@app.delete("/backups/{backup_id}")
async def delete_backup(backup_id: str):
    """Delete a backup job."""
    global backups
    backup = next((b for b in backups if b.id == backup_id), None)
    if not backup:
        raise HTTPException(status_code=404, detail="Backup not found")

    backups = [b for b in backups if b.id != backup_id]
    return {"deleted": True, "backup_id": backup_id}


@app.get("/restores", response_model=List[RestoreJob])
async def list_restores():
    """List all restore jobs."""
    return restores


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
