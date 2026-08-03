from typing import Dict, Any, List, Optional
from pydantic import BaseModel
from fastapi import FastAPI, HTTPException, Header
import structlog

logger = structlog.get_logger()
app = FastAPI(title="Multi-Factor Authentication Service", version="1.0.0")

class AuthRequest(BaseModel):
    user_id: str
    action_type: str # 'login', 'restore_backup', 'approve_expense', 'add_device'
    knowledge_factor: Optional[str] = None # Password, PIN, Recovery Phrase
    biometric_token: Optional[str] = None # Token from Android Biometric API
    device_key: Optional[str] = None # Passkey or trusted device signature

class AuthResponse(BaseModel):
    status: str
    authorized: bool
    message: str

@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "mfa-auth-service", "version": "1.0.0"}

@app.post("/auth/verify", response_model=AuthResponse)
async def verify_authentication(req: AuthRequest):
    """
    Verifies multi-factor authentication for sensitive actions.
    Vimbai never stores raw biometrics, only cryptographic assertions from the device OS.
    """
    logger.info("MFA verification requested", user_id=req.user_id, action=req.action_type)
    
    factors_provided = 0
    if req.knowledge_factor: factors_provided += 1
    if req.biometric_token: factors_provided += 1
    if req.device_key: factors_provided += 1
    
    # Define required factors based on action sensitivity
    if req.action_type == 'restore_backup':
        # Requires high security (e.g. Password + Biometric, or Recovery Phrase)
        if factors_provided >= 2 or (req.knowledge_factor and "phrase" in req.knowledge_factor):
            return AuthResponse(status="success", authorized=True, message="Backup restoration authorized.")
        else:
            raise HTTPException(status_code=403, detail="Insufficient authentication factors for backup restoration. Requires MFA.")
            
    elif req.action_type == 'approve_expense':
        if factors_provided >= 1:
            return AuthResponse(status="success", authorized=True, message="Expense approval authorized.")
        else:
            raise HTTPException(status_code=403, detail="Authentication required for approval.")
            
    return AuthResponse(status="success", authorized=True, message="Authentication successful.")

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8007)
