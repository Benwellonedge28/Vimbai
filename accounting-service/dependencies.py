from contextvars import ContextVar
from typing import Optional

from accounting_service.database import Neo4jConnector
from neo4j import AsyncSession

# Book context for request-scoped data isolation. The API gateway verifies
# X-Book-ID membership before a request reaches this service; the middleware
# in main.py binds the incoming X-Book-ID here so every Cypher query can
# scope its records by Book (None = personal / unscoped data).
book_id_var: ContextVar[Optional[str]] = ContextVar("book_id", default=None)


async def get_db_session() -> AsyncSession:
    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j", default_access_mode="w")  # Default to write access
    try:
        yield session
    finally:
        await session.close()


import os
from typing import Optional

from accounting_service.exceptions import UnauthorizedError
from fastapi import Depends, Header
from jose import JWTError, jwt


async def get_jwt_token(authorization: Optional[str] = Header(None)) -> Optional[str]:
    """Extract the Bearer token from the Authorization header, if present."""
    if authorization and authorization.startswith("Bearer "):
        return authorization.split(" ", 1)[1]
    return None


async def get_user_id(token: Optional[str] = Depends(get_jwt_token)) -> str:
    """Resolve the calling user from the JWT injected by the API gateway."""
    if token is None:
        raise UnauthorizedError(detail="Missing bearer token", code="MISSING_TOKEN")
    secret = os.environ.get("JWT_SECRET")
    if not secret:
        raise UnauthorizedError(detail="Service not configured", code="JWT_SECRET_MISSING")
    try:
        payload = jwt.decode(token, secret, algorithms=["HS256"])
    except JWTError:
        raise UnauthorizedError(detail="Could not validate credentials", code="INVALID_CREDENTIALS")
    user_id = payload.get("user_id")
    if not user_id:
        raise UnauthorizedError(detail="Token carries no user identity", code="NO_USER_ID")
    return str(user_id)
