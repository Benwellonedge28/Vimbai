from contextvars import ContextVar
from typing import Optional

from fastapi import Depends, Request, status
from multimodal_pipeline_service.exceptions import UnauthorizedError  # NEW
from multimodal_pipeline_service.utils.auth import get_current_user_claims
from neo4j import AsyncSession

# Request-scoped Book context (X-Book-ID verified by the API gateway).
# None means personal / unscoped requests.
book_id_var: ContextVar[Optional[str]] = ContextVar("book_id", default=None)


# This dependency extracts the JWT token from the request for internal service calls
async def get_jwt_token(request: Request, claims: dict = Depends(get_current_user_claims)) -> str:
    auth_header = request.headers.get("Authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        raise UnauthorizedError(
            detail="JWT token missing or invalid format.", code="MISSING_OR_INVALID_TOKEN_FORMAT"
        )  # MODIFIED
    return auth_header.split(" ")[1]


async def get_db_session() -> AsyncSession:
    """Yield a Neo4j async session for the request."""
    from multimodal_pipeline_service.database import Neo4jConnector

    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j")
    try:
        yield session
    finally:
        await session.close()


async def get_user_id(claims: dict = Depends(get_current_user_claims)) -> str:
    return claims["user_id"]
