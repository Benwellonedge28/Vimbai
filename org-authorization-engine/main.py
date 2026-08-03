from typing import Dict, Any, List, Optional
from pydantic import BaseModel
from fastapi import FastAPI, HTTPException
import structlog

logger = structlog.get_logger()
app = FastAPI(title="Org Authorization Engine (RBAC+ABAC)", version="1.0.0")

class UserContext(BaseModel):
    user_id: str
    role: str # RBAC: CEO, CFO, Manager, Employee, etc.
    department: str # ABAC attribute
    location: str # ABAC attribute

class ResourceContext(BaseModel):
    resource_type: str # e.g., 'expense', 'budget'
    amount: float # ABAC attribute
    department: str # ABAC attribute
    project: str # ABAC attribute

class AuthorizationRequest(BaseModel):
    user: UserContext
    action: str # e.g., 'approve', 'view', 'create'
    resource: ResourceContext

class AuthorizationResponse(BaseModel):
    authorized: bool
    reason: str
    required_approvals: Optional[List[str]] = None

@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "org-authorization-engine", "version": "1.0.0"}

@app.post("/authorize", response_model=AuthorizationResponse)
async def evaluate_authorization(req: AuthorizationRequest):
    """
    Evaluates complex organizational rules combining RBAC and ABAC.
    """
    logger.info("Evaluating authorization", user=req.user.user_id, action=req.action, resource=req.resource.resource_type)
    
    # Example ABAC rules
    if req.action == "approve" and req.resource.resource_type == "expense":
        # Rule 1: Amount threshold
        if req.resource.amount > 10000:
            if req.user.role not in ["CFO", "CEO"]:
                return AuthorizationResponse(
                    authorized=False, 
                    reason="Expenses above $10,000 require CFO or CEO approval.",
                    required_approvals=["CFO"]
                )
        
        # Rule 2: Department boundary
        if req.user.role == "Department Manager":
            if req.user.department != req.resource.department:
                return AuthorizationResponse(
                    authorized=False,
                    reason="Managers can only approve expenses for their own department."
                )
                
        return AuthorizationResponse(authorized=True, reason="Authorization granted based on RBAC and ABAC rules.")

    return AuthorizationResponse(authorized=False, reason="Default deny.")

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8008)
