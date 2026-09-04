import os
from functools import wraps

import httpx
import jwt
from fastapi import Depends, HTTPException, status
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer

security = HTTPBearer(auto_error=False)

IDENTITY_SERVICE_URL = os.getenv("IDENTITY_SERVICE_URL", "http://identity-service:8080")
JWT_SECRET = os.environ["JWT_SECRET"]


async def get_user_id_from_token(token: str) -> str:
    """Extract user_id from JWT token"""
    try:
        payload = jwt.decode(token, JWT_SECRET, algorithms=["HS256"])
        user_id = payload.get("sub") or payload.get("user_id")
        if not user_id:
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token: user_id not found")
        return user_id
    except jwt.ExpiredSignatureError:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Token has expired")
    except jwt.InvalidTokenError:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")


def check_permission(permission: str):
    """Dependency for checking permissions"""

    async def permission_checker(credentials: HTTPAuthorizationCredentials = Depends(security)) -> bool:
        if not credentials:
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Not authenticated")

        token = credentials.credentials
        try:
            payload = jwt.decode(token, JWT_SECRET, algorithms=["HS256"])
            user_permissions = payload.get("permissions", [])
            user_role = payload.get("role", "")

            # SUPER_ADMIN has all permissions
            if user_role == "SUPER_ADMIN":
                return True

            # Check permission array
            if permission in user_permissions or f"{permission.split('.')[0]}.*" in user_permissions:
                return True

            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN, detail=f"Permission denied: {permission} required"
            )
        except jwt.InvalidTokenError:
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")

    return permission_checker


async def get_jwt_token(credentials: HTTPAuthorizationCredentials = Depends(security)) -> str:
    """Extract JWT token for service-to-service calls"""
    if not credentials:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Not authenticated")
    return credentials.credentials
