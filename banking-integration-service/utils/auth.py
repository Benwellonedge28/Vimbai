import os
from fastapi import Depends, status, Request
from fastapi.security import OAuth2PasswordBearer
from jose import jwt, JWTError
from banking_integration_service.exceptions import UnauthorizedError, ForbiddenError

JWT_SECRET = os.environ["JWT_SECRET"]
ALGORITHM = "HS256"

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="http://localhost:8081/identity/login")

async def get_current_user_claims(token: str = Depends(oauth2_scheme)):
    credentials_exception = UnauthorizedError(detail="Could not validate credentials", code="INVALID_CREDENTIALS")
    try:
        payload = jwt.decode(token, JWT_SECRET, algorithms=[ALGORITHM])
        user_id: str = payload.get("user_id")
        username: str = payload.get("username")
        user_role: str = payload.get("role")
        permissions: list[str] = payload.get("permissions", [])
        
        if user_id is None or username is None or user_role is None:
            raise credentials_exception
        
        return {
            "user_id": user_id,
            "username": username,
            "role": user_role,
            "permissions": permissions
        }
    except JWTError:
        raise credentials_exception

def check_permission(required_permission: str):
    def permission_checker(claims: dict = Depends(get_current_user_claims)):
        user_permissions = claims["permissions"]
        user_role = claims["role"]

        if "*.*" in user_permissions or user_role == "SUPER_ADMIN":
            return

        service_prefix = required_permission.split('.')[0] + ".*"
        if service_prefix in user_permissions:
            return

        if required_permission not in user_permissions:
            raise ForbiddenError(detail=f"Not enough permissions. Requires: {required_permission}", code="INSUFFICIENT_PERMISSIONS")
    return permission_checker
