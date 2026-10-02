import os

import jwt
from fastapi import Depends, HTTPException, status
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer

security = HTTPBearer(auto_error=False)
# os.environ["JWT_SECRET"] is read from the environment at call time, not import time
# (lets tests override it and keeps import side-effect free)


def check_permission(permission: str):
    async def checker(credentials: HTTPAuthorizationCredentials = Depends(security)):
        if not credentials:
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Not authenticated")
        try:
            payload = jwt.decode(credentials.credentials, os.environ["JWT_SECRET"], algorithms=["HS256"])
            if payload.get("role") == "SUPER_ADMIN":
                return True
            if permission in payload.get("permissions", []):
                return True
            raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail=f"Permission denied: {permission}")
        except jwt.InvalidTokenError:
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")

    return Depends(checker)
