import os
from fastapi import HTTPException, status, Depends
from fastapi.security import OAuth2PasswordBearer
from jose import jwt, JWTError

# This should match the secret key used in the Identity Service
# In a real app, this should be loaded from env vars (e.g., JWT_SECRET)
JWT_SECRET = os.getenv("JWT_SECRET", "your_super_secret_jwt_key") 
ALGORITHM = "HS256"

# This is a placeholder for actual user/permission roles.
# In a real microservices setup, this service would cache user permissions
# or make a call to the Identity Service for more granular checks.
# For now, we'll extract permissions from the JWT claims.

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="http://localhost:8080/login") # Point to Identity Service login

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

        # Check for SUPER_ADMIN wildcard
        if "*.*" in user_permissions or user_role == "SUPER_ADMIN":
            return

        # Check for service-level wildcard (e.g., "accounting.*")
        service_prefix = required_permission.split('.')[0] + ".*"
        if service_prefix in user_permissions:
            return

        # Check for exact permission
        if required_permission not in user_permissions:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail=f"Not enough permissions. Requires: {required_permission}",
            )
    return permission_checker
