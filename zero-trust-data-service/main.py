from typing import Any, Dict, Optional

import structlog
from fastapi import FastAPI, Header, HTTPException
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Zero-Trust Data Service", version="1.0.0")


class EncryptedBlob(BaseModel):
    user_id: str
    data_type: str  # e.g., 'transaction', 'budget', 'report'
    encrypted_payload: str
    iv: str
    auth_tag: str


class SyncResponse(BaseModel):
    status: str
    message: str


# In-memory mock database for encrypted blobs
# The server stores ONLY what it needs to operate: Account identity, Subscription status, Auth info, Encrypted user data blobs.
# The server CANNOT read: Income details, Expenses, Financial reports, Private documents.
DB = {}


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "zero-trust-data-service", "version": "1.0.0"}


@app.post("/sync/push", response_model=SyncResponse)
async def push_encrypted_data(blob: EncryptedBlob, authorization: Optional[str] = Header(None)):
    if not authorization:
        raise HTTPException(status_code=401, detail="Unauthorized")

    logger.info("Received encrypted blob for cloud sync", user_id=blob.user_id, data_type=blob.data_type)

    if blob.user_id not in DB:
        DB[blob.user_id] = []

    DB[blob.user_id].append(blob.dict())

    return SyncResponse(
        status="success", message="Encrypted data synced to server successfully. Server cannot read contents."
    )


@app.get("/sync/pull/{user_id}", response_model=Dict[str, Any])
async def pull_encrypted_data(user_id: str, authorization: Optional[str] = Header(None)):
    if not authorization:
        raise HTTPException(status_code=401, detail="Unauthorized")

    logger.info("Serving encrypted blobs to device", user_id=user_id)

    data = DB.get(user_id, [])
    return {"user_id": user_id, "encrypted_blobs": data, "note": "Decryption must occur on-device"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8002)
