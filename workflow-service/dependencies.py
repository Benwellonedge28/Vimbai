from fastapi import Depends, HTTPException, status
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from neo4j import AsyncSession
from typing import Optional
import os
import httpx
import jwt
from datetime import datetime

security = HTTPBearer(auto_error=False)

# Environment configuration
IDENTITY_SERVICE_URL = os.getenv("IDENTITY_SERVICE_URL", "http://identity-service:8080")

async def get_db_session() -> AsyncSession:
    """Dependency for database session"""
    from workflow_service.database import Neo4jConnector
    driver = Neo4jConnector.get_driver()
    async with driver.session() as session:
        yield session

async def get_user_id(credentials: Optional[HTTPAuthorizationCredentials] = Depends(security)) -> str:
    """Extract user ID from JWT token"""
    if not credentials:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Not authenticated",
            headers={"WWW-Authenticate": "Bearer"},
        )

    try:
        token = credentials.credentials
        JWT_SECRET = os.getenv("JWT_SECRET", "your_super_secret_jwt_key")

        payload = jwt.decode(token, JWT_SECRET, algorithms=["HS256"])
        user_id = payload.get("sub") or payload.get("user_id")

        if not user_id:
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Invalid token: user_id not found"
            )

        return user_id
    except jwt.ExpiredSignatureError:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Token has expired"
        )
    except jwt.InvalidTokenError as e:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail=f"Invalid token: {str(e)}"
        )

async def check_permission(permission: str):
    """Dependency factory for checking permissions"""
    async def permission_checker(
        credentials: Optional[HTTPAuthorizationCredentials] = Depends(security),
        db_session: AsyncSession = Depends(get_db_session)
    ) -> bool:
        if not credentials:
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Not authenticated"
            )

        try:
            token = credentials.credentials
            JWT_SECRET = os.getenv("JWT_SECRET", "your_super_secret_jwt_key")
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
                status_code=status.HTTP_403_FORBIDDEN,
                detail=f"Permission denied: {permission} required"
            )
        except jwt.InvalidTokenError:
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Invalid token"
            )

    return Depends(permission_checker)

async def get_jwt_token(credentials: Optional[HTTPAuthorizationCredentials] = Depends(security)) -> str:
    """Extract JWT token for service-to-service communication"""
    if not credentials:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Not authenticated"
        )
    return credentials.credentials
