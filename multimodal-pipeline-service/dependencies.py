from fastapi import Depends, Request, status
from multimodal_pipeline_service.exceptions import UnauthorizedError  # NEW
from multimodal_pipeline_service.utils.auth import get_current_user_claims


# This dependency extracts the JWT token from the request for internal service calls
async def get_jwt_token(request: Request, claims: dict = Depends(get_current_user_claims)) -> str:
    auth_header = request.headers.get("Authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        raise UnauthorizedError(
            detail="JWT token missing or invalid format.", code="MISSING_OR_INVALID_TOKEN_FORMAT"
        )  # MODIFIED
    return auth_header.split(" ")[1]
