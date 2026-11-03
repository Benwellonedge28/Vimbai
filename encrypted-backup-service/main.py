"""
Vimbai Encrypted Backup Service
Handles encrypted backup and restore operations for financial data.
"""

import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "encrypted-backup-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8006"))

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

# Loaded bare (uvicorn main:app) and by path (root integration tests), so this
# directory has to be importable before the store module is pulled in.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import encrypted_backup_store as store  # noqa: E402

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


# Backup and restore jobs are durable records: Neo4j, caller-owned, Book-gated.
BACKUPS = ("BackupJob", "OWNS_BACKUP")
RESTORES = ("RestoreJob", "OWNS_RESTORE")


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the caller and Book context (set by the gateway) to the store."""
    store.user_id_var.set(request.headers.get("X-User-Id"))
    store.book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


def _backup_model(node: Dict[str, Any]) -> BackupJob:
    return BackupJob(
        id=node["id"],
        service_name=node.get("service_name", ""),
        backup_type=node.get("backup_type", "full"),
        status=node.get("status", "pending"),
        file_path=node.get("file_path", ""),
        encryption_key_id=node.get("encryption_key_id", ""),
        size_bytes=int(node.get("size_bytes") or 0),
        checksum=node.get("checksum", ""),
        created_at=node.get("created_at"),
        completed_at=node.get("completed_at"),
        created_by=node.get("created_by", ""),
    )


def _restore_model(node: Dict[str, Any]) -> RestoreJob:
    return RestoreJob(
        id=node["id"],
        backup_id=node.get("backup_id", ""),
        status=node.get("status", "pending"),
        restored_by=node.get("restored_by", ""),
        created_at=node.get("created_at"),
        completed_at=node.get("completed_at"),
    )


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

    node = await store.create_node(
        BACKUPS[0],
        BACKUPS[1],
        {
            "id": job.id,
            "service_name": job.service_name,
            "backup_type": job.backup_type,
            "status": job.status,
            "file_path": job.file_path,
            "encryption_key_id": job.encryption_key_id,
            "size_bytes": job.size_bytes,
            "checksum": job.checksum,
            "created_at": job.created_at.isoformat(),
            "completed_at": job.completed_at.isoformat(),
            "created_by": job.created_by,
        },
    )
    logger.info("Backup completed", backup_id=job.id, service=request.service_name, type=request.backup_type)
    return _backup_model(node)


@app.get("/backups", response_model=List[BackupJob])
async def list_backups(service_name: Optional[str] = None, status: Optional[str] = None):
    """List backup jobs with optional filters."""
    nodes = await store.list_nodes(BACKUPS[0], BACKUPS[1], {"service_name": service_name, "status": status})
    return [_backup_model(node) for node in nodes]


@app.get("/backups/{backup_id}", response_model=BackupJob)
async def get_backup(backup_id: str):
    """Get a specific backup job."""
    node = await store.get_node(BACKUPS[0], BACKUPS[1], backup_id)
    if not node:
        raise HTTPException(status_code=404, detail="Backup not found")
    return _backup_model(node)


@app.post("/backup/{backup_id}/restore", response_model=RestoreJob)
async def restore_backup(backup_id: str, restored_by: str = ""):
    """Restore from a backup job."""
    node = await store.get_node(BACKUPS[0], BACKUPS[1], backup_id)
    if not node:
        raise HTTPException(status_code=404, detail="Backup not found")
    backup = _backup_model(node)
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
    restore_node = await store.create_node(
        RESTORES[0],
        RESTORES[1],
        {
            "id": restore.id,
            "backup_id": restore.backup_id,
            "status": restore.status,
            "restored_by": restore.restored_by,
            "created_at": restore.created_at.isoformat(),
            "completed_at": restore.completed_at.isoformat(),
        },
    )
    logger.info("Restore completed", restore_id=restore.id, backup_id=backup_id)
    return _restore_model(restore_node)


@app.delete("/backups/{backup_id}")
async def delete_backup(backup_id: str):
    """Delete a backup job."""
    node = await store.get_node(BACKUPS[0], BACKUPS[1], backup_id)
    if not node:
        raise HTTPException(status_code=404, detail="Backup not found")

    await store.delete_node(BACKUPS[0], BACKUPS[1], backup_id)
    return {"deleted": True, "backup_id": backup_id}


@app.get("/restores", response_model=List[RestoreJob])
async def list_restores():
    """List all restore jobs."""
    nodes = await store.list_nodes(RESTORES[0], RESTORES[1])
    return [_restore_model(node) for node in nodes]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
