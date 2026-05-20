import os
from fastapi import HTTPException, status, Depends
from fastapi.security import OAuth2PasswordBearer
from jose import jwt, JWTError

JWT_SECRET = os.getenv("JWT_SECRET", "your_super_secret_jwt_key")
ALGORITHM = "HS256"

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="http://localhost:8080/login")

async def get_current_user_claims(token: str = Depends(oauth2_scheme)):
    credentials_exception = HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Could not validate credentials",
        headers={"WWW-Authenticate": "Bearer"},
    )
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
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail=f"Not enough permissions. Requires: {required_permission}",
            )
    return permission_checker
