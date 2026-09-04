"""FastAPI dependencies for NPO Service"""

import os
from contextvars import ContextVar
from typing import Optional

import httpx
from fastapi import Header, HTTPException, status

# API Gateway URL for auth validation
API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# Request-scoped Book context (X-Book-ID verified by the API gateway).
# None means personal / unscoped requests.
book_id_var: ContextVar[Optional[str]] = ContextVar("book_id", default=None)


async def get_user_id(x_user_id: str = Header(...)) -> str:
    """Extract user ID from header"""
    return x_user_id


async def get_jwt_token(authorization: str = Header(...)) -> str:
    """Extract JWT token from authorization header"""
    if not authorization.startswith("Bearer "):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid authorization header format")
    return authorization[7:]  # Remove "Bearer " prefix


async def validate_jwt_token(token: str) -> dict:
    """Validate JWT token via API Gateway"""
    try:
        async with httpx.AsyncClient() as client:
            response = await client.get(
                f"{API_GATEWAY_URL}/auth/validate", headers={"Authorization": f"Bearer {token}"}
            )
            if response.status_code == 200:
                return response.json()
            else:
                raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")
    except httpx.RequestError:
        # If API Gateway is unavailable, allow request (dev mode)
        return {"user_id": "unknown", "valid": True}


def check_permission(permission: str):
    """Create dependency to check user permission"""

    async def permission_checker(token: str = Header(...), x_user_id: str = Header(...)):
        # In production, validate token and check permissions via API Gateway
        # For now, we'll validate the token exists
        if not token:
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Authentication required")
        return {"user_id": x_user_id, "permission": permission}

    return permission_checker
