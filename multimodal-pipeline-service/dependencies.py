from fastapi import Depends, HTTPException, status, Request
from multimodal_pipeline_service.utils.auth import get_current_user_claims

# This dependency extracts the JWT token from the request for internal service calls
async def get_jwt_token(request: Request, claims: dict = Depends(get_current_user_claims)) -> str:
    auth_header = request.headers.get("Authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="JWT token missing or invalid format")
    return auth_header.split(" ")[1]
